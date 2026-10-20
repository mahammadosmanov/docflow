// Port of src-tauri/src/registry.rs: the closed operation registry that the
// frontend renders forms from. Every form is generated from these ParamSchemas.
library;

import 'contracts.dart';

/// Operation ID of the Fast RFI Scan (`operations::process_scans::FAST_COVER_OPERATION_ID`).
/// The process-scans module owns the constant; it is mirrored here so the
/// registry can be built without that module.
const String fastCoverOperationId = 'rfi.process_scans_fast_covers';

const List<String> _paperChoices = ['auto', 'A4', 'A3', 'A5', 'Letter', 'Legal'];
const List<String> _orientationChoices = ['auto', 'portrait', 'landscape'];
const List<String> _scalingChoices = ['fit', 'actual', 'shrink'];
const List<String> _colorChoices = ['printer', 'color', 'grayscale'];
const List<String> _duplexChoices = ['printer', 'simplex', 'long', 'short'];

/// Port of `RegistryFacts`: host facts that shape the schemas (default
/// workbook, installed printers). Printer names are the only host input.
final class RegistryFacts {
  const RegistryFacts({
    this.defaultWorkbook,
    this.printers = const <String>[],
    this.defaultPrinter,
  });

  final String? defaultWorkbook;
  final List<String> printers;
  final String? defaultPrinter;
}

Map<String, String> _labels(List<(String, String)> values) => {
  for (final (value, label) in values) value: label,
};

Map<String, String> _prefixedLabels(String prefix, List<String> values) => {
  for (final value in values) value: '$prefix.$value',
};

ParamDef _parameter({
  required String name,
  required ParamKind kind,
  required String labelKey,
  required Object? defaultValue,
  required bool isRequired,
  List<String> choices = const <String>[],
  Map<String, String> choiceLabels = const <String, String>{},
  JsonObject visibleWhen = const <String, Object?>{},
  String? helpKey,
  bool remember = true,
}) => ParamDef(
  name: name,
  kind: kind,
  labelKey: labelKey,
  defaultValue: defaultValue,
  required: isRequired,
  choices: choices,
  choiceLabels: choiceLabels,
  visibleWhen: visibleWhen,
  helpKey: helpKey,
  remember: remember,
);

/// `transient_parameter`: a parameter that is never remembered in settings.
ParamDef _transientParameter({
  required String name,
  required ParamKind kind,
  required String labelKey,
  required Object? defaultValue,
  String? helpKey,
}) => _parameter(
  name: name,
  kind: kind,
  labelKey: labelKey,
  defaultValue: defaultValue,
  isRequired: false,
  helpKey: helpKey,
  remember: false,
);

OperationDef _operation(String id, List<ParamDef> schema) => OperationDef(
  id: id,
  titleKey: 'op.$id.title',
  descriptionKey: 'op.$id.desc',
  schema: ParamSchema(params: schema),
);

/// Port of `operation_registry`. The result order is the frontend order.
List<OperationDef> operationRegistry(RegistryFacts facts) {
  final fileGroup = _operation('rfi.group_files', [
    _parameter(
      name: 'source',
      kind: ParamKind.folder,
      labelKey: 'param.rfi.fileGroup.source',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.rfi.fileGroup.source.help',
    ),
    _parameter(
      name: 'destination',
      kind: ParamKind.folder,
      labelKey: 'param.rfi.fileGroup.destination',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.rfi.fileGroup.destination.help',
    ),
  ]);

  final rfiLog = _operation('rfi.log_add', [
    _parameter(
      name: 'workbook',
      kind: ParamKind.file,
      labelKey: 'param.rfi.workbook',
      defaultValue: facts.defaultWorkbook,
      isRequired: true,
      helpKey: 'param.rfi.workbook.help',
    ),
    _parameter(
      name: 'email_text',
      kind: ParamKind.longText,
      labelKey: 'param.rfi.emailText',
      defaultValue: '',
      isRequired: false,
      helpKey: 'param.rfi.emailText.help',
    ),
    _parameter(
      name: 'email_provider',
      kind: ParamKind.choice,
      labelKey: 'param.rfi.emailProvider',
      defaultValue: 'offline',
      isRequired: true,
      choices: const ['offline', 'gemini'],
      choiceLabels: _labels([
        ('offline', 'choice.rfi.emailProvider.offline'),
        ('gemini', 'choice.rfi.emailProvider.gemini'),
      ]),
      helpKey: 'param.rfi.emailProvider.help',
    ),
    _parameter(
      name: 'gemini_api_key',
      kind: ParamKind.secret,
      labelKey: 'param.rfi.geminiApiKey',
      defaultValue: '',
      isRequired: false,
      visibleWhen: const {'email_provider': 'gemini'},
      helpKey: 'param.rfi.geminiApiKey.help',
    ),
    _parameter(
      name: 'sheet_counts',
      kind: ParamKind.sheetCounts,
      labelKey: 'param.rfi.sheetCounts',
      defaultValue: const {'CIV': 3, 'MEC': 0, 'ELE': 0, 'PRC': 0},
      isRequired: true,
      choices: const ['CIV', 'MEC', 'ELE', 'PRC'],
      helpKey: 'param.rfi.sheetCounts.help',
    ),
    _parameter(
      name: 'overflow_next_day',
      kind: ParamKind.bool,
      labelKey: 'param.rfi.overflowNextDay',
      defaultValue: true,
      isRequired: false,
      helpKey: 'param.rfi.overflowNextDay.help',
    ),
  ]);

  final rfiFollowUp = _operation('rfi.follow_up_sync', [
    _parameter(
      name: 'rfi_log_workbook',
      kind: ParamKind.file,
      labelKey: 'param.rfi.followUp.rfiLogWorkbook',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.rfi.followUp.rfiLogWorkbook.help',
    ),
    _parameter(
      name: 'follow_up_workbook',
      kind: ParamKind.file,
      labelKey: 'param.rfi.followUp.followUpWorkbook',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.rfi.followUp.followUpWorkbook.help',
    ),
  ]);

  final concreteDocuments = _operation('rfi.generate_concrete_documents', [
    _parameter(
      name: 'rfi_log_workbook',
      kind: ParamKind.file,
      labelKey: 'param.rfi.documents.rfiLog',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.rfi.documents.rfiLog.help',
    ),
    _parameter(
      name: 'follow_up_workbook',
      kind: ParamKind.file,
      labelKey: 'param.rfi.documents.followUp',
      defaultValue: null,
      isRequired: false,
      helpKey: 'param.rfi.documents.followUp.help',
    ),
    _parameter(
      name: 'template_010',
      kind: ParamKind.file,
      labelKey: 'param.rfi.documents.template010',
      defaultValue: null,
      isRequired: false,
    ),
    _parameter(
      name: 'template_011',
      kind: ParamKind.file,
      labelKey: 'param.rfi.documents.template011',
      defaultValue: null,
      isRequired: false,
    ),
    _parameter(
      name: 'template_013',
      kind: ParamKind.file,
      labelKey: 'param.rfi.documents.template013',
      defaultValue: null,
      isRequired: false,
    ),
    _parameter(
      name: 'template_014',
      kind: ParamKind.file,
      labelKey: 'param.rfi.documents.template014',
      defaultValue: null,
      isRequired: false,
    ),
    _parameter(
      name: 'output_directory',
      kind: ParamKind.folder,
      labelKey: 'param.rfi.documents.output',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.rfi.documents.output.help',
    ),
    _parameter(
      name: 'rfis_010',
      kind: ParamKind.longText,
      labelKey: 'param.rfi.documents.rfis010',
      defaultValue: '',
      isRequired: false,
      helpKey: 'param.rfi.documents.rfis.help',
    ),
    _parameter(
      name: 'rfis_011',
      kind: ParamKind.longText,
      labelKey: 'param.rfi.documents.rfis011',
      defaultValue: '',
      isRequired: false,
      helpKey: 'param.rfi.documents.rfis.help',
    ),
    _parameter(
      name: 'rfis_013',
      kind: ParamKind.longText,
      labelKey: 'param.rfi.documents.rfis013',
      defaultValue: '',
      isRequired: false,
      helpKey: 'param.rfi.documents.rfis.help',
    ),
    _parameter(
      name: 'rfis_014',
      kind: ParamKind.longText,
      labelKey: 'param.rfi.documents.rfis014',
      defaultValue: '',
      isRequired: false,
      helpKey: 'param.rfi.documents.rfis.help',
    ),
    _parameter(
      name: 'water_cement_ratios',
      kind: ParamKind.longText,
      labelKey: 'param.rfi.documents.waterRatios',
      defaultValue: '',
      isRequired: false,
      helpKey: 'param.rfi.documents.waterRatios.help',
    ),
    _parameter(
      name: 'ambient_min',
      kind: ParamKind.int,
      labelKey: 'param.rfi.documents.ambientMin',
      defaultValue: 19,
      isRequired: false,
    ),
    _parameter(
      name: 'ambient_groups',
      kind: ParamKind.longText,
      labelKey: 'param.rfi.documents.ambientGroups',
      defaultValue: '',
      isRequired: false,
      helpKey: 'param.rfi.documents.ambientGroups.help',
    ),
    _parameter(
      name: 'ambient_max',
      kind: ParamKind.int,
      labelKey: 'param.rfi.documents.ambientMax',
      defaultValue: 24,
      isRequired: false,
    ),
  ]);

  final rfiSystemSync = _operation('rfi.system_sync', [
    _parameter(
      name: 'rfi_system_root',
      kind: ParamKind.folder,
      labelKey: 'param.rfi.systemSync.rfiSystemRoot',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.rfi.systemSync.rfiSystemRoot.help',
    ),
    _parameter(
      name: 'rfi_log_workbook',
      kind: ParamKind.file,
      labelKey: 'param.rfi.systemSync.rfiLogWorkbook',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.rfi.systemSync.rfiLogWorkbook.help',
    ),
    _parameter(
      name: 'follow_up_workbook',
      kind: ParamKind.file,
      labelKey: 'param.rfi.systemSync.followUpWorkbook',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.rfi.systemSync.followUpWorkbook.help',
    ),
    _transientParameter(
      name: 'force_rescan',
      kind: ParamKind.bool,
      labelKey: 'param.scanIndex.forceRescan',
      defaultValue: false,
      helpKey: 'param.scanIndex.forceRescan.help',
    ),
  ]);

  final rfiCompleteStatusSync = _operation('rfi.complete_status_sync', [
    _parameter(
      name: 'rfi_system_root',
      kind: ParamKind.folder,
      labelKey: 'param.rfi.completeStatus.rfiSystemRoot',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.rfi.completeStatus.rfiSystemRoot.help',
    ),
    _parameter(
      name: 'rfi_log_workbook',
      kind: ParamKind.file,
      labelKey: 'param.rfi.completeStatus.rfiLogWorkbook',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.rfi.completeStatus.rfiLogWorkbook.help',
    ),
  ]);

  final defaultPrinter = facts.defaultPrinter ??
      (facts.printers.isEmpty ? null : facts.printers.first);

  // A previous file selection is task identity, not a reusable setting. The
  // operator must explicitly select files for every new print procedure.
  final batchPrintFiles = _parameter(
    name: 'files',
    kind: ParamKind.files,
    labelKey: 'param.print.files',
    defaultValue: const <Object?>[],
    isRequired: true,
    helpKey: 'param.print.files.help',
    remember: false,
  );
  final batchPrint = OperationDef(
    id: 'files.batch_print',
    titleKey: 'op.files.batch_print.title',
    descriptionKey: 'op.files.batch_print.desc',
    schema: ParamSchema(
      params: [
        batchPrintFiles,
        _transientParameter(
          name: 'rfi_system_root',
          kind: ParamKind.folder,
          labelKey: 'param.print.rfiRoot',
          defaultValue: null,
          helpKey: 'param.print.rfiRoot.help',
        ),
        _transientParameter(
          name: 'rfi_numbers',
          kind: ParamKind.longText,
          labelKey: 'param.print.rfiNumbers',
          defaultValue: '',
          helpKey: 'param.print.rfiNumbers.help',
        ),
        _transientParameter(
          name: 'include_first_page',
          kind: ParamKind.bool,
          labelKey: 'param.print.includeFirstPage',
          defaultValue: true,
        ),
        _transientParameter(
          name: 'include_sketch',
          kind: ParamKind.bool,
          labelKey: 'param.print.includeSketch',
          defaultValue: true,
        ),
        _parameter(
          name: 'printer',
          kind: ParamKind.choice,
          labelKey: 'param.print.printer',
          defaultValue: defaultPrinter,
          isRequired: true,
          choices: List<String>.of(facts.printers),
          helpKey: 'param.print.printer.help',
        ),
        _parameter(
          name: 'paper',
          kind: ParamKind.choice,
          labelKey: 'param.print.paper',
          defaultValue: 'A4',
          isRequired: true,
          choices: _paperChoices,
          choiceLabels: _prefixedLabels('choice.print.paper', _paperChoices),
          helpKey: 'param.print.paper.help',
        ),
        _parameter(
          name: 'orientation',
          kind: ParamKind.choice,
          labelKey: 'param.print.orientation',
          defaultValue: 'auto',
          isRequired: true,
          choices: _orientationChoices,
          choiceLabels: _prefixedLabels(
            'choice.print.orientation',
            _orientationChoices,
          ),
        ),
        _parameter(
          name: 'scaling',
          kind: ParamKind.choice,
          labelKey: 'param.print.scaling',
          defaultValue: 'fit',
          isRequired: true,
          choices: _scalingChoices,
          choiceLabels: _prefixedLabels('choice.print.scaling', _scalingChoices),
        ),
        _parameter(
          name: 'color',
          kind: ParamKind.choice,
          labelKey: 'param.print.color',
          defaultValue: 'printer',
          isRequired: true,
          choices: _colorChoices,
          choiceLabels: _prefixedLabels('choice.print.color', _colorChoices),
        ),
        _parameter(
          name: 'duplex',
          kind: ParamKind.choice,
          labelKey: 'param.print.duplex',
          defaultValue: 'printer',
          isRequired: true,
          choices: _duplexChoices,
          choiceLabels: _prefixedLabels('choice.print.duplex', _duplexChoices),
        ),
        _parameter(
          name: 'copies',
          kind: ParamKind.int,
          labelKey: 'param.print.copies',
          defaultValue: 1,
          isRequired: true,
          helpKey: 'param.print.copies.help',
        ),
        _parameter(
          name: 'collate',
          kind: ParamKind.bool,
          labelKey: 'param.print.collate',
          defaultValue: true,
          isRequired: false,
          helpKey: 'param.print.collate.help',
        ),
        _parameter(
          name: 'retries',
          kind: ParamKind.int,
          labelKey: 'param.print.retries',
          defaultValue: 0,
          isRequired: true,
          helpKey: 'param.print.retries.help',
        ),
        _parameter(
          name: 'on_error',
          kind: ParamKind.choice,
          labelKey: 'param.print.onError',
          defaultValue: 'pause',
          isRequired: true,
          choices: const ['pause', 'skip'],
          choiceLabels: _labels([
            ('pause', 'choice.print.onError.pause'),
            ('skip', 'choice.print.onError.skip'),
          ]),
          helpKey: 'param.print.onError.help',
        ),
        _parameter(
          name: 'timeout_minutes',
          kind: ParamKind.int,
          labelKey: 'param.print.timeout',
          defaultValue: 30,
          isRequired: true,
          helpKey: 'param.print.timeout.help',
        ),
      ],
    ),
    supportsPresets: true,
  );

  final stageSignedPages = _operation('rfi.stage_signed_first_pages', [
    _parameter(
      name: 'signed_scans',
      kind: ParamKind.folder,
      labelKey: 'param.rfi.signedScans',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.rfi.signedScans.help',
    ),
    _parameter(
      name: 'archive_root',
      kind: ParamKind.folder,
      labelKey: 'param.rfi.archiveRoot',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.rfi.archiveRoot.help',
    ),
    _parameter(
      name: 'review_root',
      kind: ParamKind.folder,
      labelKey: 'param.rfi.reviewRoot',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.rfi.reviewRoot.help',
    ),
    _parameter(
      name: 'scan_subfolders',
      kind: ParamKind.bool,
      labelKey: 'param.rfi.scanSubfolders',
      defaultValue: true,
      isRequired: false,
      helpKey: 'param.rfi.scanSubfolders.help',
    ),
  ]);

  final normalizePdfOrientation = _operation('rfi.normalize_pdf_orientation', [
    _parameter(
      name: 'rfi_system_root',
      kind: ParamKind.folder,
      labelKey: 'param.rfi.orientation.rfiSystemRoot',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.rfi.orientation.rfiSystemRoot.help',
    ),
    _parameter(
      name: 'review_root',
      kind: ParamKind.folder,
      labelKey: 'param.rfi.orientation.reviewRoot',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.rfi.orientation.reviewRoot.help',
    ),
    _transientParameter(
      name: 'force_rescan',
      kind: ParamKind.bool,
      labelKey: 'param.scanIndex.forceRescan',
      defaultValue: false,
      helpKey: 'param.scanIndex.forceRescan.help',
    ),
  ]);

  final fastCoverScans = _operation(fastCoverOperationId, [
    _parameter(
      name: 'source',
      kind: ParamKind.folder,
      labelKey: 'param.source',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.source.help',
    ),
    _parameter(
      name: 'dest',
      kind: ParamKind.folder,
      labelKey: 'param.dest',
      defaultValue: null,
      isRequired: true,
      helpKey: 'param.dest.help',
    ),
    _parameter(
      name: 'scan_mode',
      kind: ParamKind.choice,
      labelKey: 'param.rfi.processScans.scanMode',
      defaultValue: 'multi_rfi',
      isRequired: true,
      choices: const ['multi_rfi', 'single_rfi'],
      choiceLabels: _labels([
        ('multi_rfi', 'choice.rfi.processScans.scanMode.multiRfi'),
        ('single_rfi', 'choice.rfi.processScans.scanMode.singleRfi'),
      ]),
      helpKey: 'param.rfi.processScans.scanMode.help',
    ),
    _parameter(
      name: 'organize_into_rfi_folders',
      kind: ParamKind.bool,
      labelKey: 'param.organize_into_rfi_folders',
      defaultValue: true,
      isRequired: false,
      helpKey: 'param.organize_into_rfi_folders.help',
    ),
    _parameter(
      name: 'keep_original',
      kind: ParamKind.bool,
      labelKey: 'param.keep_original',
      defaultValue: false,
      isRequired: false,
    ),
    _parameter(
      name: 'correct_orientation',
      kind: ParamKind.bool,
      labelKey: 'param.rfi.processScans.correctOrientation',
      defaultValue: false,
      isRequired: false,
      visibleWhen: const {'scan_mode': 'multi_rfi'},
      helpKey: 'param.rfi.processScans.correctOrientation.help',
    ),
  ]);

  return [
    fileGroup,
    rfiLog,
    rfiFollowUp,
    concreteDocuments,
    batchPrint,
    stageSignedPages,
    normalizePdfOrientation,
    rfiSystemSync,
    rfiCompleteStatusSync,
    fastCoverScans,
  ];
}
