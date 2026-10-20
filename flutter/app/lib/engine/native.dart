// Port of the native command router (src-tauri/src/native.rs `NativeBackend`).
// Composition, path materialization, apply/cancel/undo, print queue, recovery
// and cloud sync are ported here. Preview and review operations whose Dart
// modules still need value decoders are not ported yet: they fail with
// `not_ported` (see [_notPorted]).

import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:path/path.dart' as p;
import 'package:uuid/uuid.dart';

import 'concrete_document_sources.dart'
    show
        CivilRecord,
        ConcreteDocumentError,
        hasConCiv010Form,
        readCivilRecords,
        readFollowUpRecords,
        requestedRfis;
import 'concrete_documents.dart'
    show
        ConcreteBatch,
        DocumentType,
        GeneratedFile,
        SourceRecord,
        generateConcreteDocuments;
import 'ocr.dart';
import 'review.dart' as review;
import 'operations/rfi_file_group.dart' as rfi_file_group;
import 'clipboard.dart' show PlatformClipboardAdapter;
import 'platform/windows/outlook.dart' show ReplyContent;
import 'operations/rfi_print_find.dart' as rfi_find;
import 'ooxml.dart' show maxPackageBytes;
import 'operations/replica_publish.dart' as replica_publish;
import 'operations/pdf_orientation.dart' as scans_orientation;
import 'operations/stage_signed_pages.dart' as scans_stage;
import 'rfi_log.dart' as rfi_log;
import 'operations/process_scans.dart' as scans;
import 'pdf.dart' as pdf_lib;

import 'auth.dart' as auth;
import 'cap_fs.dart' as cap;
import 'backend.dart';
import 'cloud.dart';
import 'contracts.dart';
import 'errors.dart';
import 'engine.dart' show ProgressHandler;
import 'fs_ops.dart';
import 'gemini.dart';
import 'http.dart';
import 'journal.dart';
import 'outlook_reply_draft.dart';
import 'paths.dart';
import 'path_selection.dart';
import 'plan_store.dart';
import 'platform/excel.dart';
import 'platform/excel_windows_contract.dart';
import 'platform/windows/excel.dart';
import 'platform/windows/printing.dart' show WindowsControlledPdfSubmitter;
import 'print_queue.dart';
import 'production_ports.dart';
import '../models/progress.dart' as models;
import 'progress.dart';
import 'registry.dart';
import 'rfi_follow_up.dart' as rfi_follow_up;
import 'rfi_system_sync.dart' as rfi_system_sync;
import 'scan_index.dart';
import 'settings.dart';
import 'undo.dart' as undo_lib;
import 'platform/mod.dart' show productionPrintAdapter;
import 'diagnostics.dart' as diag;
import 'platform/windows/excel.dart' show Win32ComDispatchBackend;
import 'platform/windows/outlook.dart'
    show OutlookAdapter, OutlookFailure, OutlookFailureKind, WindowsOutlookAdapter;

const String _geminiUrl = 'https://generativelanguage.googleapis.com/v1beta';
const String _geminiDefaultModel = 'gemini-3.1-flash-lite';
const List<String> _geminiFallbackModels = [
  'gemini-3.5-flash-lite',
  'gemini-3.5-flash',
];
const String _reviewOperationId = 'rfi.review';
const String _batchPrintOperationId = 'files.batch_print';
const String _replicaPublishOperationId = 'files.replica_publish';
const Duration _authorityTtl = Duration(minutes: 30);

/// Native command router. Shares one [PathSelectionStore] with the router.
final class NativeBackend implements NativeOperations {
  NativeBackend._({
    required this.selections,
    required this.settings,
    required this.authService,
    required this.cloud,
    required this.gemini,
    required this.journals,
    required this.filesystem,
    required this.print,
    required this.outlook,
    required this.scanIndex,
    required this.appDataDir,
  });

  /// Composition root (`NativeBackend::production`). [authConfig] is the
  /// Supabase endpoint shared with [authService]; [appDataDir] holds the
  /// settings, journal and trash directories.
  factory NativeBackend.production({
    required PathSelectionStore selections,
    required SettingsStore settings,
    required auth.AuthService authService,
    required auth.AuthConfig authConfig,
    required String appDataDir,
  }) {
    final trash = Directory(p.join(appDataDir, 'trash'));
    try {
      trash.createSync(recursive: true);
    } on FileSystemException {
      // Best effort, as in Rust (`let _ = fs::create_dir_all`).
    }
    final ScanIndex scanIndex;
    try {
      scanIndex = ScanIndex.open(p.join(appDataDir, 'scan-index.sqlite3'));
    } on Object catch (error) {
      throw BackendError.protocol('scan index initialization: $error');
    }
    final journalDir = p.join(appDataDir, 'journal');
    final supabaseHttp = HttpClient(const HttpConfig());
    final cloudHistory = CloudHistory(
      authService,
      supabaseHttp,
      authConfig,
      Directory(journalDir),
      const JournalUndoPolicy(),
    );
    final geminiHttp = HttpClient(
      const HttpConfig(requestTimeout: Duration(seconds: 45)),
    );
    final gemini = GeminiClient(
      geminiHttp,
      _geminiUrl,
      _configuredGeminiModels(),
    );
    final mutationLock = GlobalMutationLock();
    final filesystem = FilesystemAuthority(
      journalDir: journalDir,
      ttl: _authorityTtl,
      ooxml: const ProductionFollowUpOoxml(),
      rfiLog: const ProductionRfiLogWorkbook(),
      rfiSystemSync: const rfi_system_sync.RfiSystemSync(),
      pdf: const ProductionPdfEngine(),
      replicaPublish: const ProductionReplicaPublish(),
      undoJournal: undo_lib.undoJournal,
      mutationLock: mutationLock,
    );
    final print = PrintService(
      queueRoot: defaultQueueRoot(appDataDir),
      journalRoot: journalDir,
      ttl: _authorityTtl,
      mutationLock: mutationLock,
      adapter: productionPrintAdapter(),
      excelPipeline: productionExcelPipeline(),
    );
    final outlook = OutlookDraftService(
      journalDir: journalDir,
      ttl: _authorityTtl,
      mutationLock: mutationLock,
      adapter: _productionOutlookAdapter(),
    );
    return NativeBackend._(
      selections: selections,
      settings: settings,
      authService: authService,
      cloud: cloudHistory,
      gemini: gemini,
      journals: JournalStore(journalDir),
      filesystem: filesystem,
      print: print,
      outlook: outlook,
      scanIndex: scanIndex,
      appDataDir: appDataDir,
    );
  }

  final PathSelectionStore selections;
  final SettingsStore settings;
  final auth.AuthService authService;
  final GeminiClient gemini;
  final JournalStore journals;
  final FilesystemAuthority filesystem;
  final PrintService print;
  final OutlookDraftService outlook;
  final ScanIndex scanIndex;
  final String appDataDir;

  /// Cloud history for the router's `history:` seam.
  final CloudHistory cloud;

  static List<String> _configuredGeminiModels() {
    final configured = Platform.environment['DOCFLOW_GEMINI_MODEL'] ?? '';
    final models = <String>[];
    for (final model in [
      configured,
      _geminiDefaultModel,
      ..._geminiFallbackModels,
    ]) {
      if (model.isNotEmpty && !models.contains(model)) models.add(model);
    }
    return models;
  }

  /// Adapts the router's model progress callback to the engine's events.
  static void Function(ProgressEvent) _progressBridge(
    ProgressHandler? handler,
  ) =>
      (event) => handler?.call(
        models.ProgressEvent(
          phase: event.phase,
          done: event.done,
          total: event.total,
          subject: event.subject,
        ),
      );

  /// `production_outlook_adapter`: Outlook COM on Windows, fail-closed elsewhere.
  static OutlookAdapter _productionOutlookAdapter() => Platform.isWindows
      ? WindowsOutlookAdapter(backendFactory: () => Win32ComDispatchBackend())
      : const UnsupportedOutlookAdapter();

  static int _now() => DateTime.now().millisecondsSinceEpoch ~/ 1000;

  static Never _notPorted(String what) =>
      throw BackendError('not_ported', '$what is not ported yet');

  @override
  RegistryFacts registryFacts() {
    try {
      final inventory = print.inventory();
      return RegistryFacts(
        printers: inventory.printers,
        defaultPrinter: inventory.defaultPrinter,
      );
    } on Object {
      return const RegistryFacts();
    }
  }

  @override
  diag.PrinterDiagnostic printerDiagnostics(CallerIdentity? caller) {
    final value = print.diagnostics(caller);
    final queue = value.latestQueue;
    return diag.PrinterDiagnostic(
      inventoryStatus: value.inventoryStatus,
      printerCount: value.printerCount,
      defaultPrinterAvailable: value.defaultPrinterAvailable,
      latestQueue: queue == null
          ? null
          : diag.PrintQueueDiagnostic(
              queueStatus: queue.queueStatus,
              printer: queue.printer,
              fileCount: queue.fileCount,
              counts: queue.counts,
              jobs: [
                for (final job in queue.jobs)
                  diag.PrintJobDiagnostic(
                    format: job.format,
                    route: job.route,
                    status: job.status,
                    attempts: job.attempts,
                    paper: job.paper,
                    orientation: job.orientation,
                    scaling: job.scaling,
                    color: job.color,
                    duplex: job.duplex,
                    pageRange: job.pageRange,
                    errorDetail: job.errorDetail,
                  ),
              ],
            ),
    );
  }

  // ---- Path materialization (native.rs materialize_preview) --------------

  List<ParamDef> _pathParameters(String opId) {
    final matches = operationRegistry(const RegistryFacts())
        .where((operation) => operation.id == opId);
    if (matches.isEmpty) {
      throw BackendError.validation('unknown operation ID');
    }
    return [
      for (final param in matches.first.schema.params)
        if (param.kind == ParamKind.folder ||
            param.kind == ParamKind.file ||
            param.kind == ParamKind.files)
          param,
    ];
  }

  String _resolveSelection(
    Object? raw,
    CallerIdentity caller,
    String opId,
    String name,
    SelectionKind kind,
  ) {
    if (raw is! Map) throw BackendError.invalidParams();
    final PathSelection selection;
    try {
      selection = PathSelection.fromJson(Map<String, Object?>.from(raw));
    } on Object {
      throw BackendError.invalidParams();
    }
    return selections.resolve(selection, caller, opId, name, kind);
  }

  /// `materialize_preview`: swaps selection references in `values` for the
  /// granted paths. Other params are left untouched.
  JsonObject _materializePreview(CallerIdentity caller, JsonObject params) {
    final opId = params['opId'];
    final raw = params['values'];
    if (opId is! String || raw is! Map) throw BackendError.invalidParams();
    final values = Map<String, Object?>.from(raw);
    for (final param in _pathParameters(opId)) {
      final value = values[param.name];
      if (value == null) continue;
      switch (param.kind) {
        case ParamKind.folder:
          values[param.name] = _resolveSelection(
            value,
            caller,
            opId,
            param.name,
            SelectionKind.folder,
          );
        case ParamKind.file:
          values[param.name] = _resolveSelection(
            value,
            caller,
            opId,
            param.name,
            SelectionKind.file,
          );
        case ParamKind.files:
          if (value is! List) throw BackendError.invalidParams();
          values[param.name] = [
            for (final item in value)
              _resolveSelection(
                item,
                caller,
                opId,
                param.name,
                SelectionKind.file,
              ),
          ];
        default:
          break;
      }
    }
    return {...params, 'values': values};
  }

  // ---- Preview -------------------------------------------------------------

  @override
  Future<Object?> preview(
    CallerIdentity caller,
    String opId,
    JsonObject params,
    ProgressHandler? onProgress,
  ) async {
    final progress = _progressBridge(onProgress);
    final materialized = _materializePreview(caller, params);
    final values = materialized['values']! as Map<String, Object?>;
    switch (opId) {
      case _batchPrintOperationId:
        final BatchPrintParams batch;
        try {
          batch = BatchPrintParams.fromJson(values);
        } on Object {
          throw BackendError.invalidParams();
        }
        return print.preview(caller, batch, progress).toJson();
      case outlookReplyOperationId:
        final replyText = values['replyText'];
        if (replyText is! String) throw BackendError.invalidParams();
        progress(
          const ProgressEvent(
            phase: 'outlook_selection',
            done: 0,
            total: 1,
            subject: '',
          ),
        );
        final plan = outlook.preview(caller, replyText);
        progress(
          const ProgressEvent(
            phase: 'preview_ready',
            done: 1,
            total: 1,
            subject: '',
          ),
        );
        return plan.toJson();
      case 'rfi.stage_signed_first_pages':
        return (await _previewStageSigned(caller, values, onProgress)).toJson();
      case 'rfi.normalize_pdf_orientation':
        return (await _previewOrientation(caller, values, onProgress)).toJson();
      case outlookArchiveOperationId:
        return _previewOutlookArchive(caller, values, onProgress).toJson();
      case 'rfi.group_files':
        return (await _previewRfiFileGroup(caller, values, onProgress)).toJson();
      case 'rfi.generate_concrete_documents':
        return (await _previewConcrete(caller, values, onProgress)).toJson();
      case 'files.replica_publish':
        return (await _previewReplica(caller, values, onProgress)).toJson();
      case 'rfi.system_sync':
        return (await _previewSystemSync(caller, values, onProgress)).toJson();
      case 'rfi.complete_status_sync':
        return (await _previewCompleteStatus(
          caller,
          values,
          onProgress,
        )).toJson();
      case rfi_follow_up.followUpOperationId:
        return (await _previewFollowUp(caller, values, onProgress)).toJson();
      case 'rfi.log_add':
        return (await _previewRfiLog(caller, values, onProgress)).toJson();
      case 'rfi.process_scans_fast_covers':
        return (await _previewProcessScans(
          caller,
          values,
          onProgress,
        )).toJson();
      default:
        // RFI file group/log, concrete documents, follow-up, system sync,
        // complete status, stage signed pages, PDF orientation, Outlook
        // archive and replica publish: pending ports.
        _notPorted('preview $opId');
    }
  }

  // ---- Shared helpers ------------------------------------------------------

  GrantScope _scope(CallerIdentity caller, String opId) => GrantScope.create(
    caller.ownerId,
    caller.sessionId,
    const Uuid().v4(),
    opId,
  );

  Plan _cacheOwned(
    CallerIdentity caller,
    String opId,
    List<String> grantIds,
    Plan plan,
  ) => filesystem.cachePlan(
    BackendOrigin.rust,
    PlanBinding(
      ownerId: caller.ownerId,
      sessionId: caller.sessionId,
      operationId: opId,
      grantIds: grantIds,
    ),
    plan,
  );

  static String _str(JsonObject values, String key) {
    final value = values[key];
    if (value is! String) throw BackendError.invalidParams();
    return value;
  }

  static bool _flag(JsonObject values, String key) => values[key] == true;

  String get _trashDir => p.join(appDataDir, 'trash');

  // ---- Process scans (fast covers) ---------------------------------------

  Future<Plan> _previewProcessScans(
    CallerIdentity caller,
    JsonObject values,
    ProgressHandler? onProgress,
  ) async {
    const opId = 'rfi.process_scans_fast_covers';
    final scope = _scope(caller, opId);
    final source = _str(values, 'source');
    final destination = _str(values, 'dest');
    final keepOriginal = _flag(values, 'keep_original');
    final scanModeWire = values['scan_mode'];
    final scanMode = scanModeWire == 'single_rfi'
        ? scans.ScanMode.singleRfi
        : scans.ScanMode.multiRfi;
    final sourceGrant = filesystem.issueGrant(
      scope,
      source,
      GrantAccess.readWrite,
    );
    final destinationGrant = filesystem.issueGrant(
      scope,
      destination,
      GrantAccess.readWrite,
    );
    final trashGrant = keepOriginal
        ? null
        : filesystem.issueGrant(scope, _trashDir, GrantAccess.readWrite);
    final params = scans.ProcessScansParams(
      source: source,
      destination: destination,
      scanMode: scanMode,
      organizeIntoRfiFolders: _flag(values, 'organize_into_rfi_folders'),
      keepOriginal: keepOriginal,
      correctOrientation: _flag(values, 'correct_orientation'),
      trashRoot: keepOriginal ? null : _trashDir,
    );
    final (token, registration) = filesystem.registerCancellation(opId);
    try {
      final plan = await scans.previewFastCovers(
        params: params,
        ocr: NativeOcr.discover(),
        inspector: const _PdfInspectorBridge(),
        progress: _progressBridge(onProgress),
        cancelled: () => token.isCancelled,
      );
      return _cacheOwned(caller, opId, [
        sourceGrant.grantId,
        destinationGrant.grantId,
        if (trashGrant != null) trashGrant.grantId,
      ], plan);
    } finally {
      registration.close();
    }
  }

  // ---- RFI system sync and completion status ----------------------------

  /// `fs::canonicalize` with the Rust error key in the message.
  static String _canonical(String path, String message) {
    try {
      return File(path).resolveSymbolicLinksSync();
    } on FileSystemException catch (error) {
      throw BackendError.validation('$message: ${error.message}');
    }
  }

  /// Opens a granted anchor's workbook read-write and reads it into a
  /// follow-up file. The handle is closed before returning.
  static rfi_follow_up.FollowUpWorkbookFile _readMutableWorkbook(
    cap.AnchoredPath anchor,
    String path,
    String lockedKey,
  ) {
    final cap.OsHandle handle;
    try {
      handle = anchor.openMutable();
    } on Object {
      throw BackendError.validation(lockedKey);
    }
    try {
      return rfi_system_sync.workbookFileFromHandle(handle, path);
    } finally {
      handle.close();
    }
  }

  static bool _within(String root, String path) =>
      p.equals(path, root) || p.isWithin(root, path);

  Future<Plan> _previewSystemSync(
    CallerIdentity caller,
    JsonObject values,
    ProgressHandler? onProgress,
  ) async {
    const opId = 'rfi.system_sync';
    final systemRoot = _canonical(
      _str(values, 'rfi_system_root'),
      'error.rfi.systemSync.systemRootInvalid: cannot resolve root',
    );
    final sourcePath = _canonical(
      _str(values, 'rfi_log_workbook'),
      'error.rfi.systemSync.workbookInvalid: cannot resolve RFI Log',
    );
    final targetPath = _canonical(
      _str(values, 'follow_up_workbook'),
      'error.rfi.systemSync.workbookInvalid: cannot resolve Follow Up',
    );
    final sourceRoot = p.dirname(sourcePath);
    final targetRoot = p.dirname(targetPath);
    final systemNeedsWrite =
        _within(systemRoot, sourcePath) || _within(systemRoot, targetPath);
    final requests = <(String, GrantAccess)>[
      (systemRoot, systemNeedsWrite ? GrantAccess.readWrite : GrantAccess.read),
    ];
    final sourceCovered = systemNeedsWrite && _within(systemRoot, sourcePath);
    if (!sourceCovered) requests.add((sourceRoot, GrantAccess.readWrite));
    final targetCovered =
        (systemNeedsWrite && _within(systemRoot, targetPath)) ||
        (!sourceCovered && _within(sourceRoot, targetPath));
    if (!targetCovered) requests.add((targetRoot, GrantAccess.readWrite));
    final provisional = filesystem.issueProvisionalGrants(
      _scope(caller, opId),
      requests,
    );
    try {
      final sourceGrant = provisional.grants.firstWhere(
        (grant) =>
            grant.access.allows(PathIntent.write) &&
            _within(grant.root, sourcePath),
        orElse: () => throw BackendError(
          'path_invalid',
          'No write grant covering RFI Log',
        ),
      );
      final targetGrant = provisional.grants.firstWhere(
        (grant) =>
            grant.access.allows(PathIntent.write) &&
            _within(grant.root, targetPath),
        orElse: () => throw BackendError(
          'path_invalid',
          'No write grant covering Follow Up',
        ),
      );
      final sourceAnchor = filesystem.anchorGrantedPath(
        sourceGrant,
        sourcePath,
        PathIntent.write,
      );
      final targetAnchor = filesystem.anchorGrantedPath(
        targetGrant,
        targetPath,
        PathIntent.write,
      );
      if (sourceAnchor.sameFileAs(targetAnchor)) {
        throw BackendError.validation(
          'RFI Log and Follow Up workbooks must be different files',
        );
      }
      final source = _readMutableWorkbook(
        sourceAnchor,
        sourcePath,
        'error.rfi.systemSync.workbookLocked: cannot open RFI Log',
      );
      final target = _readMutableWorkbook(
        targetAnchor,
        targetPath,
        'error.rfi.systemSync.workbookLocked',
      );
      var plan = await rfi_system_sync.previewSystemSyncIndexed(
        systemRoot,
        sourcePath,
        targetPath,
        source,
        target,
        scanIndex,
        caller.ownerId,
        _flag(values, 'force_rescan'),
        _progressBridge(onProgress),
      );
      final reservation = filesystem.reservePlanId();
      plan = reservation.assignToEmpty(plan);
      final cached = filesystem.cacheReservedPlan(
        BackendOrigin.rust,
        PlanBinding(
          ownerId: caller.ownerId,
          sessionId: caller.sessionId,
          operationId: opId,
          grantIds: [for (final grant in provisional.grants) grant.grantId],
        ),
        reservation,
        plan,
      );
      provisional.commit();
      return cached;
    } finally {
      provisional.revokeUnlessCommitted();
    }
  }

  Future<Plan> _previewCompleteStatus(
    CallerIdentity caller,
    JsonObject values,
    ProgressHandler? onProgress,
  ) async {
    const opId = 'rfi.complete_status_sync';
    final systemRoot = _canonical(
      _str(values, 'rfi_system_root'),
      'error.rfi.completeStatus.systemRootInvalid: cannot resolve root',
    );
    final workbookPath = _canonical(
      _str(values, 'rfi_log_workbook'),
      'error.rfi.completeStatus.workbookInvalid: cannot resolve RFI Log',
    );
    final inSystem = _within(systemRoot, workbookPath);
    final requests = <(String, GrantAccess)>[
      (systemRoot, inSystem ? GrantAccess.readWrite : GrantAccess.read),
      if (!inSystem) (p.dirname(workbookPath), GrantAccess.readWrite),
    ];
    final provisional = filesystem.issueProvisionalGrants(
      _scope(caller, opId),
      requests,
    );
    try {
      final grant = provisional.grants.firstWhere(
        (grant) =>
            grant.access.allows(PathIntent.write) &&
            _within(grant.root, workbookPath),
        orElse: () => throw BackendError(
          'path_invalid',
          'No write grant covering RFI Log',
        ),
      );
      final anchor = filesystem.anchorGrantedPath(
        grant,
        workbookPath,
        PathIntent.write,
      );
      final workbook = _readMutableWorkbook(
        anchor,
        workbookPath,
        'error.rfi.completeStatus.workbookLocked',
      );
      final (token, registration) = filesystem.registerCancellation(opId);
      try {
        var plan = await rfi_system_sync.previewCompleteStatusSync(
          systemRoot,
          workbookPath,
          workbook,
          _progressBridge(onProgress),
          () => token.isCancelled,
        );
        final reservation = filesystem.reservePlanId();
        plan = reservation.assignToEmpty(plan);
        final cached = filesystem.cacheReservedPlan(
          BackendOrigin.rust,
          PlanBinding(
            ownerId: caller.ownerId,
            sessionId: caller.sessionId,
            operationId: opId,
            grantIds: [for (final grant in provisional.grants) grant.grantId],
          ),
          reservation,
          plan,
        );
        provisional.commit();
        return cached;
      } finally {
        registration.close();
      }
    } finally {
      provisional.revokeUnlessCommitted();
    }
  }

  // ---- Signed pages, orientation, Outlook archive --------------------------

  Future<Plan> _previewStageSigned(
    CallerIdentity caller,
    JsonObject values,
    ProgressHandler? onProgress,
  ) async {
    const opId = 'rfi.stage_signed_first_pages';
    final scope = _scope(caller, opId);
    final signed = filesystem.issueGrant(
      scope,
      _str(values, 'signed_scans'),
      GrantAccess.read,
    );
    final archive = filesystem.issueGrant(
      scope,
      _str(values, 'archive_root'),
      GrantAccess.read,
    );
    final review = filesystem.issueGrant(
      scope,
      _str(values, 'review_root'),
      GrantAccess.readWrite,
    );
    final (token, registration) = filesystem.registerCancellation(opId);
    try {
      final plan = await scans_stage.previewStageSignedPages(
        scans_stage.StageSignedPagesParams(
          signedScans: _str(values, 'signed_scans'),
          archiveRoot: _str(values, 'archive_root'),
          reviewRoot: _str(values, 'review_root'),
          scanSubfolders: _flag(values, 'scan_subfolders'),
        ),
        NativeOcr.discover(),
        progress: _progressBridge(onProgress),
        cancelled: () => token.isCancelled,
      );
      return _cacheOwned(caller, opId, [
        signed.grantId,
        archive.grantId,
        review.grantId,
      ], plan);
    } finally {
      registration.close();
    }
  }

  Future<Plan> _previewOrientation(
    CallerIdentity caller,
    JsonObject values,
    ProgressHandler? onProgress,
  ) async {
    const opId = 'rfi.normalize_pdf_orientation';
    final scope = _scope(caller, opId);
    final systemRoot = _str(values, 'rfi_system_root');
    final reviewRoot = _str(values, 'review_root');
    filesystem.issueGrant(scope, systemRoot, GrantAccess.read);
    filesystem.issueGrant(scope, reviewRoot, GrantAccess.readWrite);
    final (token, registration) = filesystem.registerCancellation(opId);
    try {
      final plan = await scans_orientation.previewOrientation(
        scans_orientation.PdfOrientationParams(
          rfiSystemRoot: systemRoot,
          reviewRoot: reviewRoot,
        ),
        NativeOcr.discover(),
        progress: _progressBridge(onProgress),
        cancelled: () => token.isCancelled,
        scanIndex: scanIndex,
        ownerId: caller.ownerId,
        forceRescan: _flag(values, 'force_rescan'),
      );
      // Fresh grants bind the finished plan: large trees can outlive the grant
      // TTL, and apply revalidates identities anyway (native.rs 2141).
      final source = filesystem.issueGrant(scope, systemRoot, GrantAccess.read);
      final review = filesystem.issueGrant(
        scope,
        reviewRoot,
        GrantAccess.readWrite,
      );
      return _cacheOwned(caller, opId, [source.grantId, review.grantId], plan);
    } finally {
      registration.close();
    }
  }

  Plan _previewOutlookArchive(
    CallerIdentity caller,
    JsonObject values,
    ProgressHandler? onProgress,
  ) {
    const opId = outlookArchiveOperationId;
    final destination = _str(values, 'destination');
    final provisional = filesystem.issueProvisionalGrants(
      _scope(caller, opId),
      [(destination, GrantAccess.write)],
    );
    try {
      final progress = _progressBridge(onProgress);
      progress(
        const ProgressEvent(
          phase: 'outlook_selection',
          done: 0,
          total: 1,
          subject: '',
        ),
      );
      final PreparedArchive prepared = _outlookCall(
        () => outlook.prepareArchive(caller, destination),
      );
      final reservation = filesystem.reservePlanId();
      final plan = reservation.assignToEmpty(prepared.plan);
      final cached = filesystem.cacheReservedPlan(
        BackendOrigin.rust,
        PlanBinding(
          ownerId: caller.ownerId,
          sessionId: caller.sessionId,
          operationId: opId,
          grantIds: [for (final grant in provisional.grants) grant.grantId],
        ),
        reservation,
        plan,
      );
      outlook.bindArchive(cached.planId, prepared);
      provisional.commit();
      progress(
        const ProgressEvent(
          phase: 'preview_ready',
          done: 1,
          total: 1,
          subject: '',
        ),
      );
      return cached;
    } finally {
      provisional.revokeUnlessCommitted();
    }
  }

  /// Maps an Outlook adapter failure to the backend error code.
  static T _outlookCall<T>(T Function() body) {
    try {
      return body();
    } on OutlookFailure catch (failure) {
      throw BackendError('outlook_failure', failure.key);
    }
  }

  // ---- Replica publish ---------------------------------------------------

  Future<Plan> _previewReplica(
    CallerIdentity caller,
    JsonObject values,
    ProgressHandler? onProgress,
  ) async {
    const opId = 'files.replica_publish';
    final replica_publish.ReplicaPublishParams params;
    try {
      params = replica_publish.ReplicaPublishParams.fromJson(values);
    } on Object {
      throw BackendError.invalidParams();
    }
    final roots = [
      for (final root in [params.rootA, params.rootB, params.rootC])
        _canonicalOrPath(root, 'error.replica.rootInvalid'),
    ];
    final backup = _canonicalOrPath(
      params.backupRoot,
      'error.replica.backupInvalid',
    );
    final sourceIndex = switch (params.sourceRoot) {
      'a' => 0,
      'b' => 1,
      'c' => 2,
      _ => throw BackendError.validation('error.replica.sourceInvalid'),
    };
    final grants = <(String, GrantAccess)>[
      for (var index = 0; index < roots.length; index++)
        (
          roots[index],
          index == sourceIndex ? GrantAccess.read : GrantAccess.readWrite,
        ),
      (backup, GrantAccess.readWrite),
    ];
    final provisional = filesystem.issueProvisionalGrants(
      _scope(caller, opId),
      grants,
    );
    try {
      final plan = await replica_publish.previewReplicaPublishWithIndex(
        params,
        ownerId: caller.ownerId,
        index: scanIndex,
        progress: _progressBridge(onProgress),
      );
      final cached = filesystem.cachePlan(
        BackendOrigin.rust,
        PlanBinding(
          ownerId: caller.ownerId,
          sessionId: caller.sessionId,
          operationId: opId,
          grantIds: [for (final grant in provisional.grants) grant.grantId],
        ),
        plan,
      );
      provisional.commit();
      return cached;
    } finally {
      provisional.revokeUnlessCommitted();
    }
  }

  static String _canonicalOrPath(String path, String message) {
    try {
      return File(path).resolveSymbolicLinksSync();
    } on FileSystemException {
      throw BackendError('path_invalid', message);
    }
  }

  // ---- RFI file group ----------------------------------------------------

  Future<Plan> _previewRfiFileGroup(
    CallerIdentity caller,
    JsonObject values,
    ProgressHandler? onProgress,
  ) async {
    const opId = 'rfi.group_files';
    final source = _str(values, 'source');
    final destination = _str(values, 'destination');
    final provisional = filesystem.issueProvisionalGrants(
      _scope(caller, opId),
      [(source, GrantAccess.read), (destination, GrantAccess.readWrite)],
    );
    final (token, registration) = filesystem.registerCancellation(opId);
    try {
      final plan = await rfi_file_group.previewRfiFileGroup(
        params: rfi_file_group.GroupRfiFilesParams(
          source: source,
          destination: destination,
        ),
        ocr: _RfiOcrBridge(NativeOcr.discover()),
        pdfPageCount: (path) => pdf_lib.inspect(path).pageCount,
        progress: _progressBridge(onProgress),
        cancelled: () => token.isCancelled,
      );
      final cached = _cacheOwned(caller, opId, [
        for (final grant in provisional.grants) grant.grantId,
      ], plan);
      provisional.commit();
      return cached;
    } finally {
      registration.close();
      provisional.revokeUnlessCommitted();
    }
  }

  // ---- Concrete documents ------------------------------------------------

  static BackendError _concreteError(String detail) =>
      BackendError.validation('error.rfi.documents.invalidInput: $detail');

  static List<String> _optionalRfis(String raw) {
    if (raw.trim().isEmpty) return const [];
    try {
      return requestedRfis(raw);
    } on ConcreteDocumentError catch (error) {
      throw _concreteError(error.message);
    }
  }

  static String? _optionalPath(JsonObject values, String name) {
    final value = values[name];
    if (value is! String || value.trim().isEmpty) return null;
    return value;
  }

  static String _requiredPath(JsonObject values, String name) {
    final value = _optionalPath(values, name);
    if (value == null) throw BackendError.invalidParams();
    return value;
  }

  /// `parse_ambient_groups`: `RFI[, RFI...]=MIN-MAX` per line.
  static Map<String, (int, int)> _parseAmbientGroups(
    String raw,
    List<String> selected,
  ) {
    final groups = <String, (int, int)>{};
    for (final rawLine in raw.split('\n')) {
      final line = rawLine.trim();
      if (line.isEmpty) continue;
      final equals = line.indexOf('=');
      if (equals < 0) {
        throw _concreteError(
          'Ambient groups must use `RFI[, RFI...]=MIN-MAX` per line',
        );
      }
      final range = line.substring(equals + 1).trim();
      final dash = range.indexOf('-');
      if (dash < 0) {
        throw _concreteError('Ambient ranges must use MIN-MAX');
      }
      const rangeError =
          'Ambient range values must be whole degrees from 0 to 100';
      final minimum = int.tryParse(range.substring(0, dash).trim());
      final maximum = int.tryParse(range.substring(dash + 1).trim());
      if (minimum == null ||
          maximum == null ||
          minimum < 0 ||
          maximum < 0 ||
          minimum > 255 ||
          maximum > 255) {
        throw _concreteError(rangeError);
      }
      if (maximum > 100 || minimum > maximum) {
        throw _concreteError(
          'Ambient range must be between 0 and 100 °C with minimum no greater than maximum',
        );
      }
      for (final rfi in _requestedOrThrow(line.substring(0, equals))) {
        if (!selected.contains(rfi)) {
          throw _concreteError(
            'Ambient group includes an RFI not selected for CON-CIV-011: $rfi',
          );
        }
        if (groups.containsKey(rfi)) {
          throw _concreteError(
            'Ambient range was specified more than once for $rfi',
          );
        }
        groups[rfi] = (minimum, maximum);
      }
    }
    return groups;
  }

  static List<String> _requestedOrThrow(String raw) {
    try {
      return requestedRfis(raw);
    } on ConcreteDocumentError catch (error) {
      throw _concreteError(error.message);
    }
  }

  /// `parse_water_ratios`: one `RFI = ratio` entry per line.
  static Map<String, String> _parseWaterRatios(String raw) {
    final result = <String, String>{};
    for (final rawLine in raw.split('\n')) {
      final line = rawLine.trim();
      if (line.isEmpty) continue;
      final equals = line.indexOf('=');
      if (equals < 0) {
        throw _concreteError(
          'Water/cement ratios must use one `RFI = ratio` entry per line',
        );
      }
      final normalized = _requestedOrThrow(line.substring(0, equals));
      if (normalized.length != 1) {
        throw _concreteError(
          'Each water/cement ratio line must name exactly one RFI',
        );
      }
      final rfi = normalized.single;
      final ratio = line.substring(equals + 1).trim();
      final parsed = double.tryParse(ratio);
      if (parsed == null) {
        throw _concreteError('Water/cement ratios must be numeric');
      }
      if (!parsed.isFinite || parsed <= 0.0 || parsed > 1.0) {
        throw _concreteError(
          'Water/cement ratios must be greater than 0 and no greater than 1',
        );
      }
      if (result.containsKey(rfi)) {
        throw _concreteError(
          'Water/cement ratio was entered more than once for $rfi',
        );
      }
      result[rfi] = ratio;
    }
    return result;
  }

  static String _documentLocation(DocumentType type, String area, String tag) {
    final a = area.trim();
    final t = tag.trim();
    if (a.isEmpty) return t;
    if (t.isEmpty) return a;
    return switch (type) {
      DocumentType.civ010 => '$a / $t',
      DocumentType.civ011 => '$a $t',
      DocumentType.civ013 => '$t ($a)',
      DocumentType.civ014 => '($a) $t',
    };
  }

  /// `excel_date_text`: a serial day number to `dd.mm.yyyy`.
  static String _excelDate(String value) {
    final serial = double.tryParse(value.trim());
    if (serial == null ||
        !serial.isFinite ||
        serial < 1.0 ||
        serial > 2958466.0) {
      return value;
    }
    final date = DateTime.utc(1899, 12, 30).add(Duration(days: serial.floor()));
    String two(int n) => n.toString().padLeft(2, '0');
    return '${two(date.day)}.${two(date.month)}.${date.year}';
  }

  /// `excel_time_text`: a day fraction to `HH:MM`.
  static String _excelTime(String value) {
    final serial = double.tryParse(value.trim());
    if (serial == null || !serial.isFinite || serial < 0.0 || serial >= 1.0) {
      return value;
    }
    final minutes = (serial * 24 * 60).round() % (24 * 60);
    String two(int n) => n.toString().padLeft(2, '0');
    return '${two(minutes ~/ 60)}:${two(minutes % 60)}';
  }

  /// Adds [root] once; upgrades an existing entry to read-write when asked.
  static void _addRoot(
    List<(String, GrantAccess)> roots,
    String root,
    GrantAccess access,
  ) {
    for (var index = 0; index < roots.length; index++) {
      if (roots[index].$1 == root) {
        if (access == GrantAccess.readWrite) {
          roots[index] = (root, GrantAccess.readWrite);
        }
        return;
      }
    }
    roots.add((root, access));
  }

  static Uint8List _readHandleAll(cap.OsHandle handle) {
    if (handle.metadata().size > maxPackageBytes) {
      throw _concreteError(
        'A selected Office package is larger than the supported limit',
      );
    }
    final builder = BytesBuilder(copy: false);
    final buffer = Uint8List(64 * 1024);
    handle.seek(0);
    while (true) {
      final count = handle.read(buffer);
      if (count == 0) break;
      builder.add(Uint8List.fromList(buffer.sublist(0, count)));
    }
    return builder.takeBytes();
  }

  Future<Plan> _previewConcrete(
    CallerIdentity caller,
    JsonObject values,
    ProgressHandler? onProgress,
  ) async {
    const opId = 'rfi.generate_concrete_documents';
    String text(String name) {
      final value = values[name];
      return value is String ? value.trim() : '';
    }

    final rfis010 = _optionalRfis(text('rfis_010'));
    final rfis011 = _optionalRfis(text('rfis_011'));
    final rfis013 = _optionalRfis(text('rfis_013'));
    final rfis014 = _optionalRfis(text('rfis_014'));
    if (rfis010.isEmpty &&
        rfis011.isEmpty &&
        rfis013.isEmpty &&
        rfis014.isEmpty) {
      throw _concreteError('Enter RFI numbers for at least one CON-CIV type');
    }
    final ambientMin = values['ambient_min'] is int
        ? values['ambient_min']! as int
        : 19;
    final ambientMax = values['ambient_max'] is int
        ? values['ambient_max']! as int
        : 24;
    if (ambientMin < 0 ||
        ambientMin > 100 ||
        ambientMax < 0 ||
        ambientMax > 100 ||
        ambientMin > ambientMax) {
      throw _concreteError(
        'Ambient temperature range must be between 0 and 100 °C with minimum no greater than maximum',
      );
    }
    final ambientGroups = _parseAmbientGroups(text('ambient_groups'), rfis011);
    final rfiLog = _requiredPath(values, 'rfi_log_workbook');
    final outputDirectory = _requiredPath(values, 'output_directory');
    if (!Directory(outputDirectory).existsSync()) {
      throw _concreteError('Output folder is unavailable');
    }
    final selections = <(DocumentType, List<String>, String)>[];
    for (final (type, numbers, parameter) in [
      (DocumentType.civ010, rfis010, 'template_010'),
      (DocumentType.civ011, rfis011, 'template_011'),
      (DocumentType.civ013, rfis013, 'template_013'),
      (DocumentType.civ014, rfis014, 'template_014'),
    ]) {
      if (numbers.isEmpty) continue;
      final template = _optionalPath(values, parameter);
      if (template == null) {
        throw _concreteError(
          'Select an example template for every requested document type',
        );
      }
      if (!File(template).existsSync()) {
        throw _concreteError('A selected template file is unavailable');
      }
      selections.add((type, numbers, template));
    }
    final needsFollowUp = selections.any(
      (selection) => selection.$1 == DocumentType.civ010,
    );
    final followUp = needsFollowUp
        ? _optionalPath(values, 'follow_up_workbook')
        : null;
    if (!File(rfiLog).existsSync()) {
      throw _concreteError('The selected RFI Log workbook is unavailable');
    }
    final requested = {
      for (final selection in selections) ...selection.$2,
    }.toList()..sort();

    final roots = <(String, GrantAccess)>[];
    for (final path in [
      rfiLog,
      if (followUp != null && File(followUp).existsSync()) followUp,
      for (final selection in selections) selection.$3,
    ]) {
      _addRoot(roots, p.dirname(path), GrantAccess.read);
    }
    _addRoot(roots, outputDirectory, GrantAccess.readWrite);
    final provisional = filesystem.issueProvisionalGrants(
      _scope(caller, opId),
      roots,
    );
    try {
      Uint8List readSelected(String path) {
        final covering = [
          for (final grant in provisional.grants)
            if (_within(grant.root, path) &&
                grant.access.allows(PathIntent.read))
              grant,
        ];
        if (covering.isEmpty) {
          throw BackendError.validation(
            'No read grant covers a selected input',
          );
        }
        final grant = covering.reduce(
          (best, grant) => grant.root.length > best.root.length ? grant : best,
        );
        final cap.AnchoredPath anchor;
        try {
          anchor = filesystem.anchorGrantedPath(grant, path, PathIntent.read);
        } on Object {
          throw BackendError.pathGrantUnavailable();
        }
        final cap.OsHandle handle;
        try {
          handle = anchor.openRead();
        } on Object {
          throw _concreteError('Cannot read selected input');
        }
        try {
          return _readHandleAll(handle);
        } finally {
          handle.close();
        }
      }

      final Map<String, CivilRecord> civil;
      try {
        civil = readCivilRecords(readSelected(rfiLog), requested);
      } on ConcreteDocumentError catch (error) {
        throw _concreteError(error.message);
      }
      final rfisRequiringFollowUp = [
        for (final rfi in rfis010)
          if (civil[rfi] != null && hasConCiv010Form(civil[rfi]!.forms)) rfi,
      ];
      if (rfisRequiringFollowUp.isNotEmpty) {
        if (followUp == null) {
          throw BackendError.validation(
            'error.param.required.follow_up_workbook',
          );
        }
        if (!File(followUp).existsSync()) {
          throw _concreteError(
            'The Concrete Specimen Test Follow-Up workbook is unavailable',
          );
        }
      }
      final warnings = <Warning>[
        for (final rfi in rfis010)
          if (!rfisRequiringFollowUp.contains(rfi))
            Warning(key: 'warn.rfi.documents.formNotRequired', subject: rfi),
      ];
      final Map<String, List<Map<String, String>>>? followRecords;
      if (followUp != null && rfisRequiringFollowUp.isNotEmpty) {
        try {
          followRecords = readFollowUpRecords(
            readSelected(followUp),
            rfisRequiringFollowUp,
          );
        } on ConcreteDocumentError catch (error) {
          throw _concreteError(error.message);
        }
      } else {
        followRecords = null;
      }
      final ratios = _parseWaterRatios(text('water_cement_ratios'));
      final seed = DateTime.now().microsecondsSinceEpoch;

      final generated = <GeneratedFile>[];
      for (final (type, numbers, templatePath) in selections) {
        final templateBytes = readSelected(templatePath);
        final records = <SourceRecord>[];
        for (final rfi in numbers) {
          final rec = civil[rfi];
          if (rec == null) throw _concreteError('RFI Log row is missing');
          if (type == DocumentType.civ010 && !hasConCiv010Form(rec.forms)) {
            continue;
          }
          final location = _documentLocation(type, rec.area, rec.tag);
          if (location.isEmpty) {
            throw _concreteError('The RFI Log has no usable location for $rfi');
          }
          final followRows = followRecords?[rfi];
          if (type == DocumentType.civ010 && followRows == null) {
            warnings.add(
              Warning(key: 'warn.rfi.documents.followUpMissing', subject: rfi),
            );
            continue;
          }
          final follow = (followRows != null && followRows.isNotEmpty)
              ? followRows.first
              : null;
          String field(String key) => follow?[key] ?? '';
          final additional = type == DocumentType.civ010
              ? [
                  for (final row in (followRows ?? const []).skip(1))
                    ConcreteBatch(
                      tag: _batchValue(row, 'location', field),
                      concreteClass: _batchValue(row, 'concrete_class', field),
                      supplier: _batchValue(row, 'supplier', field),
                      batchTicket: _batchValue(row, 'batch_ticket', field),
                      truck: _batchValue(row, 'truck', field),
                      quantity: _batchValue(row, 'quantity', field),
                      totalQuantity: _batchValue(row, 'total_quantity', field),
                      batchTime: _excelTime(
                        _batchValue(row, 'batch_time', field),
                      ),
                      startTime: _excelTime(
                        _batchValue(row, 'start_time', field),
                      ),
                      endTime: _excelTime(_batchValue(row, 'end_time', field)),
                      plantSlump: _batchValue(row, 'plant_slump', field),
                      inSituSlump: _batchValue(row, 'in_situ_slump', field),
                      plantTemp: _batchValue(row, 'plant_temp', field),
                      inSituTemp: _batchValue(row, 'in_situ_temp', field),
                      samples: _batchValue(row, 'samples', field),
                    ),
                ]
              : const <ConcreteBatch>[];
          records.add(
            SourceRecord(
              rfi: rfi,
              location: location,
              tag: type == DocumentType.civ010 ? field('location') : rec.tag,
              concreteClass: field('concrete_class'),
              supplier: field('supplier'),
              batchTicket: field('batch_ticket'),
              truck: field('truck'),
              quantity: field('quantity'),
              totalQuantity: field('total_quantity'),
              batchTime: _excelTime(field('batch_time')),
              pourDate: _excelDate(field('pour_date')),
              startTime: _excelTime(field('start_time')),
              endTime: _excelTime(field('end_time')),
              plantSlump: field('plant_slump'),
              inSituSlump: field('in_situ_slump'),
              plantTemp: field('plant_temp'),
              inSituTemp: field('in_situ_temp'),
              samples: field('samples'),
              waterCementRatio: ratios[rfi],
              ambientRange: ambientGroups[rfi],
              drawing: field('drawing'),
              additionalBatches: additional,
            ),
          );
        }
        if (records.isEmpty) continue;
        try {
          generated.addAll(
            generateConcreteDocuments(
              documentType: type,
              template: templateBytes,
              records: records,
              ambientRange: (ambientMin, ambientMax),
              rngSeed: seed ^ (generated.length + 1),
            ),
          );
        } on ConcreteDocumentError catch (error) {
          throw _concreteError(error.message);
        }
      }

      final steps = <Step>[];
      final destinations = <String>{};
      for (final file in generated) {
        if (p.basename(file.filename) != file.filename) {
          throw _concreteError('Generated output name is invalid');
        }
        final destination = p.join(outputDirectory, file.filename);
        if (!destinations.add(destination)) {
          throw _concreteError(
            'Two selected records produce the same output filename',
          );
        }
        if (FileSystemEntity.typeSync(destination) !=
            FileSystemEntityType.notFound) {
          warnings.add(
            Warning(
              key: 'warn.rfi.documents.outputExists',
              subject: file.filename,
            ),
          );
          continue;
        }
        steps.add(
          Step(
            kind: StepKind.write,
            src: null,
            dst: destination,
            noteKey: 'step.rfi.generatedConcreteDocument',
            meta: {'contentBase64': base64.encode(file.bytes)},
          ),
        );
      }
      final onProgressEvent = _progressBridge(onProgress);
      onProgressEvent(
        ProgressEvent(
          phase: 'concrete_document_generation',
          done: steps.length,
          total: steps.length,
          subject: outputDirectory,
        ),
      );
      final plan = Plan(
        planId: '',
        opId: opId,
        steps: steps,
        warnings: warnings,
        meta: {'generatedCount': generated.length},
      );
      final cached = _cacheOwned(caller, opId, [
        for (final grant in provisional.grants) grant.grantId,
      ], plan);
      provisional.commit();
      return cached;
    } finally {
      provisional.revokeUnlessCommitted();
    }
  }

  static String _batchValue(
    Map<String, String> row,
    String key,
    String Function(String) field,
  ) {
    final value = row[key];
    if (value != null && value.trim().isNotEmpty) return value;
    if (key == 'location' || key == 'concrete_class' || key == 'supplier') {
      return field(key);
    }
    return '';
  }

  // ---- RFI follow-up -----------------------------------------------------

  Future<Plan> _previewFollowUp(
    CallerIdentity caller,
    JsonObject values,
    ProgressHandler? onProgress,
  ) async {
    const opId = rfi_follow_up.followUpOperationId;
    final logPath = _str(values, 'rfi_log_workbook');
    final followUpPath = _str(values, 'follow_up_workbook');
    final sourceRoot = p.dirname(logPath);
    final targetRoot = p.dirname(followUpPath);
    if (sourceRoot.isEmpty) {
      throw BackendError('path_invalid', 'RFI Log workbook has no parent');
    }
    if (targetRoot.isEmpty) {
      throw BackendError('path_invalid', 'Follow Up workbook has no parent');
    }
    final progress = _progressBridge(onProgress);
    final provisional = filesystem.issueProvisionalGrants(
      _scope(caller, opId),
      [(sourceRoot, GrantAccess.read), (targetRoot, GrantAccess.readWrite)],
    );
    try {
      final sourceAnchor = filesystem.anchorGrantedPath(
        provisional.grants[0],
        logPath,
        PathIntent.read,
      );
      final targetAnchor = filesystem.anchorGrantedPath(
        provisional.grants[1],
        followUpPath,
        PathIntent.write,
      );
      if (sourceAnchor.sameFileAs(targetAnchor)) {
        throw BackendError.validation(
          'RFI Log and Follow Up workbooks must be different files',
        );
      }
      final cap.OsHandle sourceHandle;
      final cap.OsHandle targetHandle;
      try {
        sourceHandle = sourceAnchor.openRead();
      } on Object {
        throw BackendError.validation(
          'error.rfi.followUp.workbookInvalid: cannot open RFI Log',
        );
      }
      try {
        targetHandle = targetAnchor.openMutable();
      } on Object {
        sourceHandle.close();
        throw BackendError.validation('error.rfi.followUp.workbookLocked');
      }
      final rfi_follow_up.FollowUpWorkbookFile source;
      final rfi_follow_up.FollowUpWorkbookFile target;
      try {
        progress(
          const ProgressEvent(
            phase: 'follow_up_analysis',
            done: 0,
            total: 1,
            subject: '',
          ),
        );
        source = rfi_system_sync.workbookFileFromHandle(sourceHandle, logPath);
        target = rfi_system_sync.workbookFileFromHandle(
          targetHandle,
          followUpPath,
        );
      } finally {
        sourceHandle.close();
        targetHandle.close();
      }
      var plan = rfi_follow_up.previewFromOpenFiles(
        rfiLog: source,
        followUp: target,
        ooxml: const ProductionFollowUpOoxml(),
      );
      final reservation = filesystem.reservePlanId();
      plan = reservation.assignToEmpty(plan);
      rfi_follow_up.validateOwnedPlanBudget(plan);
      final cached = filesystem.cacheReservedPlan(
        BackendOrigin.rust,
        PlanBinding(
          ownerId: caller.ownerId,
          sessionId: caller.sessionId,
          operationId: opId,
          grantIds: [for (final grant in provisional.grants) grant.grantId],
        ),
        reservation,
        plan,
      );
      provisional.commit();
      progress(
        const ProgressEvent(
          phase: 'preview_ready',
          done: 1,
          total: 1,
          subject: '',
        ),
      );
      return cached;
    } finally {
      provisional.revokeUnlessCommitted();
    }
  }

  // ---- RFI log -----------------------------------------------------------

  Future<Plan> _previewRfiLog(
    CallerIdentity caller,
    JsonObject values,
    ProgressHandler? onProgress,
  ) async {
    const opId = 'rfi.log_add';
    final workbook = values['workbook'];
    if (workbook is! String || workbook.isEmpty) {
      throw BackendError.invalidParams();
    }
    final root = p.dirname(workbook);
    if (root.isEmpty) {
      throw BackendError('path_invalid', 'workbook has no parent directory');
    }
    final grant = filesystem.issueGrant(
      _scope(caller, opId),
      root,
      GrantAccess.readWrite,
    );
    final (token, registration) = filesystem.registerCancellation(opId);
    try {
      final plan = await rfi_log.preview(
        values,
        DateTime.now(),
        gemini: gemini,
        progress: _progressBridge(onProgress),
        cancelled: () => token.isCancelled,
      );
      return _cacheOwned(caller, opId, [grant.grantId], plan);
    } finally {
      registration.close();
    }
  }

  // ---- Apply / cancel ------------------------------------------------------

  @override
  Future<({JsonObject result, String approvedOpId})> apply(
    CallerIdentity caller,
    String planId,
    String? opId,
    ProgressHandler? onProgress,
  ) async {
    final progress = _progressBridge(onProgress);
    if (outlook.containsPlan(planId)) {
      final result = outlook.apply(
        planId,
        operationId: opId,
        caller: caller,
        progress: progress,
      );
      return (result: result.toJson(), approvedOpId: outlookReplyOperationId);
    }
    if (print.containsPlan(planId)) {
      if (opId != null && opId != _batchPrintOperationId) {
        throw BackendError.validation(
          'apply operation does not match the approved plan',
        );
      }
      final result = await print.apply(planId, caller, progress);
      return (result: result.toJson(), approvedOpId: _batchPrintOperationId);
    }
    final validated = filesystem.validatePlanOperation(
      planId,
      caller,
      operationId: opId,
    );
    if (validated == outlookArchiveOperationId) {
      final result = filesystem.applyGeneratedBinaryWrite(
        planId,
        caller,
        outlookArchiveOperationId,
        progress,
        () {
          try {
            return outlook.exportArchive(planId, caller);
          } on OutlookFailure catch (failure) {
            if (failure.kind == OutlookFailureKind.outcomeUnknown) {
              throw GeneratedOutcomeUnknown(failure.key);
            }
            throw GeneratedNoEffect(failure.key);
          }
        },
      );
      return (result: result.toJson(), approvedOpId: outlookArchiveOperationId);
    }
    final automationProfile = validated == _replicaPublishOperationId
        ? filesystem
              .resolveNativePlan(planId, caller)
              .plan
              .meta['automationProfile']
        : null;
    var result = filesystem.applyWithProgress(planId, caller, progress);
    if (result.failed == 0 && automationProfile != null) {
      result = _activateReplicaAutomation(result, automationProfile);
    }
    return (result: result.toJson(), approvedOpId: validated);
  }

  /// Saves the replica automation profile and enables replica sync. Any
  /// failure counts as one more failed step with `error.replica.activationFailed`.
  OperationResult _activateReplicaAutomation(
    OperationResult result,
    Object? profile,
  ) {
    var activated = true;
    try {
      settings.saveSection('replicaAutomation', {
        'profile': profile,
        'lastAttemptUnix': 0,
      });
      final preferences = settings.preferences();
      settings.savePreferences(
        AppPrefs.fromJson({
          ...preferences.toJson(),
          'replicaSyncEnabled': true,
        }),
      );
    } on Object {
      activated = false;
    }
    if (activated) return result;
    return OperationResult(
      applied: result.applied,
      failed: result.failed + 1,
      journalId: result.journalId,
      errors: [
        ...result.errors,
        const Warning(key: 'error.replica.activationFailed', subject: ''),
      ],
      meta: result.meta,
      cloudSynced: result.cloudSynced,
    );
  }

  @override
  bool cancelPlan(String planId) =>
      outlook.cancel(planId) ||
      print.cancel(planId) ||
      filesystem.cancel(planId);

  @override
  bool cancelOperation(String opId) => filesystem.cancel(opId);

  /// `sync_result_metadata`: sets `cloudSynced` on [result] after pushing the
  /// applied counts to cloud history. Mutates [result] in place.
  @override
  Future<void> syncResult(String approvedOpId, JsonObject result) async {
    final applied = (result['applied'] as int?) ?? 0;
    final failed = (result['failed'] as int?) ?? 0;
    final meta = result['meta'];
    final journalId = result['journalId'] is String
        ? result['journalId']! as String
        : (meta is Map ? meta['auditJournalId'] as String? : null) ?? '';
    int? steps;
    try {
      steps = journals.read(journalId).steps.length;
    } on Object {
      steps = null;
    }
    if (steps == null) {
      result['cloudSynced'] = false;
      return;
    }
    result['cloudSynced'] = await cloud.push(
      approvedOpId,
      applied,
      failed,
      journalId,
      '$applied/$steps',
      _now(),
    );
  }

  // ---- Print queue, recovery, session ----------------------------------------

  @override
  Future<Object?> printQueueLatest(CallerIdentity caller) async =>
      print.latest(caller)?.toJson();

  @override
  Future<Object?> printQueueDiscard(
    CallerIdentity caller,
    String queueId,
  ) async => print.discard(caller, queueId).toJson();

  @override
  Future<Object?> printQueueResume(
    CallerIdentity caller,
    String queueId,
  ) async => (await print.resume(caller, queueId)).toJson();

  @override
  Future<Object?> recovery(CallerIdentity caller) async => {
    'journals': [
      for (final entry in filesystem.recoverPending(caller)) entry.toJson(),
    ],
    'printQueue': print.latest(caller)?.toJson(),
  };

  /// `revoke_session_authority`. Path selections are revoked by the router.
  @override
  void revokeSessionAuthority(CallerIdentity caller) {
    filesystem.revoke(caller);
    print.revoke(caller);
    outlook.revoke(caller);
  }

  // ---- Undo ------------------------------------------------------------------

  /// `try_native_undo`. Returns null for a legacy journal action.
  @override
  Future<Object?> undo(CallerIdentity caller, JsonObject params) async {
    const allowedKeys = {'journalId', 'cloudId', 'destination'};
    if (params.keys.any((key) => !allowedKeys.contains(key))) {
      throw BackendError.invalidParams();
    }
    final journalId = params['journalId'];
    if (journalId is! String) throw BackendError.invalidParams();
    final cloudId = params['cloudId'] is String
        ? params['cloudId']! as String
        : null;
    final opId = journals.inspectOperation(journalId);
    final hasDestination = params.containsKey('destination');
    if (opId == outlookReplyOperationId) {
      if (hasDestination) {
        throw BackendError.validation(
          'Outlook draft undo does not accept a destination',
        );
      }
      final result = outlook.undo(journalId, caller);
      if (result.failed == 0 && cloudId != null) {
        await cloud.markUndone(cloudId, _now());
      }
      return result.toJson();
    }
    if (opId == rfi_follow_up.followUpOperationId) {
      if (hasDestination) {
        throw BackendError.validation(
          'RFI Follow Up undo does not accept a destination',
        );
      }
      final result = filesystem.undo(journalId, caller);
      if (result.failed == 0 && cloudId != null) {
        await cloud.markUndone(cloudId, _now());
      }
      return result.toJson();
    }
    final record = journals.read(journalId);
    if (record.steps.any((step) => step.kind == StepKind.print)) {
      throw BackendError.validation('print audit journals are not undoable');
    }
    if (record.steps.any((step) => !_undoableKinds.contains(step.kind))) {
      return null;
    }
    String? destination;
    if (hasDestination) {
      final raw = params['destination'];
      if (raw is! Map) throw BackendError.invalidParams();
      final PathSelection selection;
      try {
        selection = PathSelection.fromJson(Map<String, Object?>.from(raw));
      } on Object {
        throw BackendError.invalidParams();
      }
      destination = selections.resolveScope(
        selection.selectionId,
        caller,
        SelectionScope.undoDestination,
      );
    }
    final result = filesystem.undo(journalId, caller, destination: destination);
    if (result.failed == 0 && cloudId != null) {
      await cloud.markUndone(cloudId, _now());
    }
    return result.toJson();
  }

  static const Set<StepKind> _undoableKinds = {
    StepKind.mkdir,
    StepKind.move,
    StepKind.copy,
    StepKind.rename,
    StepKind.write,
    StepKind.restoreBackup,
  };

  // ---- Operations pending ports ----------------------------------------------

  static List<Object?> _rows(JsonObject params) {
    final rows = params['rows'];
    if (rows is! List) throw BackendError.invalidParams();
    return rows;
  }

  @override
  Future<Object?> revise(CallerIdentity caller, JsonObject params) async {
    final planId = _str(params, 'planId');
    final rows = _rows(params);
    if (print.containsPlan(planId)) {
      return print.revise(caller, planId, rows).toJson();
    }
    final claimed = filesystem.resolveNativePlan(planId, caller);
    final binding = claimed.binding ?? (throw BackendError.planUnavailable());
    final plan = claimed.plan;
    if (plan.opId == rfi_follow_up.followUpOperationId) {
      final (sourcePath, targetPath) = rfi_follow_up.planWorkbookPaths(plan);
      final sourceAnchor = filesystem.anchorBoundPath(
        binding,
        caller,
        sourcePath,
        PathIntent.read,
      );
      final targetAnchor = filesystem.anchorBoundPath(
        binding,
        caller,
        targetPath,
        PathIntent.write,
      );
      final cap.OsHandle sourceHandle;
      final cap.OsHandle targetHandle;
      try {
        sourceHandle = sourceAnchor.openRead();
      } on Object {
        throw BackendError.validation('error.rfi.followUp.workbookInvalid');
      }
      try {
        targetHandle = targetAnchor.openMutable();
      } on Object {
        sourceHandle.close();
        throw BackendError.validation('error.rfi.followUp.workbookLocked');
      }
      final rfi_follow_up.FollowUpWorkbookFile source;
      final rfi_follow_up.FollowUpWorkbookFile target;
      try {
        source = rfi_system_sync.workbookFileFromHandle(
          sourceHandle,
          sourcePath,
        );
        target = rfi_system_sync.workbookFileFromHandle(
          targetHandle,
          targetPath,
        );
      } finally {
        sourceHandle.close();
        targetHandle.close();
      }
      final revised = rfi_follow_up.reviseFromOpenFiles(
        plan: plan,
        submittedRows: rows.cast<JsonObject>(),
        rfiLog: source,
        followUp: target,
        ooxml: const ProductionFollowUpOoxml(),
      );
      return filesystem
          .replaceCachedPlan(binding, caller, planId, claimed.digest, revised)
          .toJson();
    }
    final payload = {'rows': rows};
    final Plan revised;
    switch (plan.opId) {
      case 'rfi.log_add':
        revised = rfi_log.revise(plan, payload);
      case 'rfi.group_files':
        revised = rfi_file_group.reviseRfiFileGroup(plan, payload);
      case 'rfi.process_scans_fast_covers':
        revised = scans.revise(plan, payload);
      case 'rfi.system_sync' || 'rfi.complete_status_sync':
        revised = rfi_system_sync.revise(plan, rows);
      case 'rfi.stage_signed_first_pages':
        revised = scans_stage.revise(plan, payload);
      default:
        throw BackendError.validation('native plan does not support revision');
    }
    return filesystem.cachePlan(BackendOrigin.rust, binding, revised).toJson();
  }

  @override
  Future<Object?> copyTable(CallerIdentity caller, JsonObject params) async {
    final claimed = filesystem.resolveNativePlan(
      _str(params, 'planId'),
      caller,
    );
    if (claimed.plan.opId != 'rfi.log_add') {
      throw BackendError.validation(
        'native plan does not contain an RFI table',
      );
    }
    final sheet = params['sheet'];
    final payload = rfi_log.copyPayload(
      claimed.plan,
      sheet: sheet is String ? sheet : null,
    );
    await const PlatformClipboardAdapter().write(payload);
    return {'count': payload.count};
  }

  @override
  Future<Object?> rfiReplyPreview(
    CallerIdentity caller,
    JsonObject params,
  ) async {
    final claimed = filesystem.resolveNativePlan(
      _str(params, 'planId'),
      caller,
    );
    if (claimed.plan.opId != 'rfi.log_add') {
      throw BackendError.validation(
        'native plan does not contain an RFI table',
      );
    }
    final journal = journals.read(_str(params, 'journalId'));
    final committed =
        journal.opId == 'rfi.log_add' &&
        journal.state == JournalState.committed &&
        journal.ownerId == caller.ownerId &&
        journal.sessionId == caller.sessionId &&
        journal.planDigest == claimed.digest;
    if (!committed) {
      throw BackendError.validation(
        'error.rfi.outlookDraft.rfiLogNotCommitted',
      );
    }
    final payload = rfi_log.replyPayload(claimed.plan);
    return outlook
        .previewContent(
          caller,
          ReplyContent(plainText: payload.plain, html: payload.html),
        )
        .toJson();
  }

  @override
  Future<Object?> reviewScan(CallerIdentity caller, JsonObject params) async {
    final destination = selections.resolveScope(
      _selection(params['destination']).selectionId,
      caller,
      SelectionScope.reviewDestination,
    );
    final items = await review.scan(
      destination,
      _ReviewOcrBridge(NativeOcr.discover()),
    );
    return [
      for (final item in items)
        {
          'selectionId': selections
              .issue(
                caller,
                _reviewOperationId,
                'item',
                item.path,
                SelectionKind.file,
              )
              .selectionId,
          'name': item.name,
          'path': item.path,
          'code': item.code,
          'confidence': item.confidence,
          'targetExists': item.targetExists,
        },
    ];
  }

  @override
  Future<Object?> reviewPreview(
    CallerIdentity caller,
    JsonObject params,
  ) async {
    final destinationSelection = _selection(params['destination']);
    final destination = selections.resolveScope(
      destinationSelection.selectionId,
      caller,
      SelectionScope.reviewDestination,
    );
    final raw = params['decisions'];
    if (raw is! List) throw BackendError.invalidParams();
    final decisions = <review.ReviewDecision>[
      for (final item in raw)
        if (item is Map && item['itemId'] is String)
          review.ReviewDecision(
            path: selections.resolveScope(
              item['itemId']! as String,
              caller,
              SelectionScope.reviewItem,
            ),
            action: review.ReviewAction.fromWire(item['action']),
            code: item['code'] is String
                ? item['code']! as String
                : (throw BackendError.invalidParams()),
          )
        else
          throw BackendError.invalidParams(),
    ];
    final scope = _scope(caller, _reviewOperationId);
    final destinationGrant = filesystem.issueGrant(
      scope,
      destination,
      GrantAccess.readWrite,
    );
    final appDataGrant = filesystem.issueGrant(
      scope,
      appDataDir,
      GrantAccess.readWrite,
    );
    final plan = review.preview(
      destination,
      _trashDir,
      decisions,
      _reviewTimestamp(DateTime.now().toUtc()),
    );
    return _cacheOwned(caller, _reviewOperationId, [
      destinationGrant.grantId,
      appDataGrant.grantId,
    ], plan).toJson();
  }

  static PathSelection _selection(Object? raw) {
    if (raw is! Map) throw BackendError.invalidParams();
    try {
      return PathSelection.fromJson(Map<String, Object?>.from(raw));
    } on Object {
      throw BackendError.invalidParams();
    }
  }

  /// `%Y%m%dT%H%M%S%6f` in UTC.
  static String _reviewTimestamp(DateTime utc) {
    String pad(int value, int width) => value.toString().padLeft(width, '0');
    return '${pad(utc.year, 4)}${pad(utc.month, 2)}${pad(utc.day, 2)}'
        'T${pad(utc.hour, 2)}${pad(utc.minute, 2)}${pad(utc.second, 2)}'
        '${pad(utc.microsecond + utc.millisecond * 1000, 6)}';
  }

  @override
  Future<Object?> findRfiPrintFiles(
    CallerIdentity caller,
    JsonObject params,
  ) async {
    final RfiPrintFindRequest request;
    try {
      request = RfiPrintFindRequest.fromJson(params);
    } on Object {
      throw BackendError.invalidParams();
    }
    final root = selections.resolve(
      request.root,
      caller,
      _batchPrintOperationId,
      'rfi_system_root',
      SelectionKind.folder,
    );
    final String canonicalRoot;
    try {
      canonicalRoot = Directory(root).resolveSymbolicLinksSync();
    } on FileSystemException {
      throw BackendError.validation('error.print.rfi.changed');
    }
    final rfi_find.Discovery discovery;
    try {
      discovery = rfi_find.discover(
        canonicalRoot,
        request.numbers,
        rfi_find.IncludeTypes(
          firstPage: request.firstPage,
          sketch: request.sketch,
        ),
      );
    } on Object {
      throw BackendError.validation('error.print.rfi.invalidRequest');
    }
    final candidates = <RfiPrintCandidate>[];
    for (final candidate in discovery.candidates) {
      final PathSelection selection;
      try {
        selection = selections.issue(
          caller,
          _batchPrintOperationId,
          'files',
          candidate.path,
          SelectionKind.file,
        );
      } on BackendError {
        throw BackendError.validation('error.print.rfi.changed');
      }
      final issuedPath = _resolveScopedRfiPrintFile(
        selection,
        caller,
        canonicalRoot,
      );
      final resolution = discovery.resolutions.firstWhere(
        (resolution) => resolution.requested == candidate.requested,
        orElse: () => throw BackendError.invalidParams(),
      );
      final selected =
          !discovery.truncated &&
          rfi_find.initiallySelected(resolution, candidate);
      final displayPath = _within(canonicalRoot, issuedPath)
          ? p.relative(issuedPath, from: canonicalRoot)
          : issuedPath;
      candidates.add(
        RfiPrintCandidate(
          requested: candidate.requested,
          rfiCode: candidate.rfiCode,
          documentType: candidate.documentType.wireName,
          displayPath: displayPath,
          selection: PathSelection(
            selectionId: selection.selectionId,
            displayPath: displayPath,
            name: selection.name,
          ),
          selected: selected,
        ),
      );
    }
    return RfiPrintFindResult(
      candidates: candidates,
      resolutions: [
        for (final resolution in discovery.resolutions)
          RfiPrintResolution(
            requested: resolution.requested,
            rfiCodes: resolution.rfiCodes,
            missingFirstPage: resolution.missingFirstPage,
            missingSketch: resolution.missingSketch,
            ambiguous: resolution.ambiguous,
          ),
      ],
      visited: discovery.visited,
      truncated: discovery.truncated,
    ).toJson();
  }

  /// `resolve_scoped_rfi_print_file`: revokes the selection on any escape.
  String _resolveScopedRfiPrintFile(
    PathSelection selection,
    CallerIdentity caller,
    String canonicalRoot,
  ) {
    final issued = selections.resolve(
      selection,
      caller,
      _batchPrintOperationId,
      'files',
      SelectionKind.file,
    );
    final String canonicalPath;
    try {
      canonicalPath = File(issued).resolveSymbolicLinksSync();
    } on FileSystemException {
      selections.revokeSelection(selection.selectionId, caller);
      throw BackendError.validation('error.print.rfi.changed');
    }
    if (canonicalPath != issued || !_within(canonicalRoot, canonicalPath)) {
      selections.revokeSelection(selection.selectionId, caller);
      throw BackendError.validation('error.print.rfi.rootEscape');
    }
    return canonicalPath;
  }
}

/// Maps pdf.dart's inspection onto the Process Scans inspection contract.
final class _PdfInspectorBridge implements scans.PdfInspector {
  const _PdfInspectorBridge();

  @override
  scans.PdfInspection inspect(String path) {
    final facts = pdf_lib.inspect(path);
    return scans.PdfInspection(
      pageCount: facts.pageCount,
      pages: [
        for (final page in facts.pages)
          scans.PdfPageFacts(rotation: page.rotation),
      ],
      signed: facts.signed,
      encrypted: facts.encrypted,
      extractionSupported: pdf_lib.extractionSupported(facts),
    );
  }
}

/// Bridges the async [NativeOcr] onto the RFI file-group OCR contract.
final class _RfiOcrBridge extends rfi_file_group.RfiOcrAdapter {
  _RfiOcrBridge(this._ocr);

  final NativeOcr _ocr;

  @override
  Future<rfi_file_group.RfiOcrRead> read(String path) =>
      _guard(() => _ocr.read(path));

  @override
  Future<rfi_file_group.RfiOcrRead> readPage(String path, int pageIndex) =>
      _guard(() => _ocr.readPage(path, pageIndex));

  @override
  String diagnostics() => _ocr.diagnostics();

  static Future<rfi_file_group.RfiOcrRead> _guard(
    Future<scans.OcrRead> Function() body,
  ) async {
    try {
      final read = await body();
      return rfi_file_group.RfiOcrRead(
        matched: read.matched,
        source: read.source,
      );
    } on scans.OcrEngineMissing {
      throw const rfi_file_group.RfiOcrEngineMissing();
    } on scans.OcrFailure catch (failure) {
      throw rfi_file_group.RfiOcrDocumentFailure(failure.message);
    }
  }
}

/// Bridges the async [NativeOcr] onto the review scan OCR contract.
final class _ReviewOcrBridge implements review.ReviewOcrAdapter {
  _ReviewOcrBridge(this._ocr);

  final NativeOcr _ocr;

  @override
  Future<review.ReviewOcrRead> read(String path) async {
    final read = await _ocr.read(path);
    return review.ReviewOcrRead(matched: read.matched, source: read.source);
  }
}

/// Production Excel export pipeline. Only Windows has Excel COM; other hosts
/// return null, so worksheet jobs fail with `error.print.excelUnavailable`.
ExcelExportPipeline? productionExcelPipeline() {
  if (!Platform.isWindows) {
    return null;
  }
  final clock = SystemMonotonicClock();
  return ExcelExportPipeline(
    policy: const AnchoredWorkbookSourcePolicy(WindowsZoneIdentifierReader()),
    exporter: WindowsExcelExporter(
      backendFactory: Win32ComDispatchBackend.new,
      clock: clock.now,
    ),
    submitter: const WindowsControlledPdfSubmitter(),
  );
}
