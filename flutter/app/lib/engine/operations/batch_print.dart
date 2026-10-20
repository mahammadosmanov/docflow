/// Port of src-tauri/src/operations/batch_print.rs: preview, revise, resume
/// planning and job validation for the batch print operation.
library;

import 'dart:io';

import 'package:path/path.dart' as p;
import 'package:uuid/uuid.dart';

import '../contracts.dart';
import '../errors.dart';
import '../ooxml.dart';
import '../page_range.dart';
import '../print_queue.dart';

const Uuid _uuid = Uuid();

const int maxFiles = 250;
const int maxCopies = 99;
const int maxRetries = 3;
const int minTimeoutMinutes = 1;
const int maxTimeoutMinutes = 240;

const List<String> _paperChoices = [
  'auto',
  'A4',
  'A3',
  'A5',
  'Letter',
  'Legal',
];
const List<String> _orientationChoices = ['auto', 'portrait', 'landscape'];
const List<String> _scalingChoices = ['fit', 'actual', 'shrink'];
const List<String> _colorChoices = ['printer', 'color', 'grayscale'];
const List<String> _duplexChoices = ['printer', 'simplex', 'long', 'short'];
const List<String> _errorActionChoices = ['pause', 'skip'];

/// Opens a workbook for discovery. Returns null when it cannot be opened,
/// which surfaces as `warn.print.workbookDiscoveryFailed`.
typedef WorkbookOpener = OpenedPrintFile? Function(String path);

Plan preview(
  BatchPrintParams params,
  PrinterInventory inventory,
  PrintAdapter adapter,
  void Function(ProgressEvent event) progress,
) => previewWithWorkbookOpener(params, inventory, adapter, progress, (path) {
  try {
    return openedPrintFileFromDisk(File(path));
  } on FileSystemException {
    return null;
  }
});

Plan previewWithWorkbookOpener(
  BatchPrintParams params,
  PrinterInventory inventory,
  PrintAdapter adapter,
  void Function(ProgressEvent event) progress,
  WorkbookOpener openWorkbook,
) {
  if (inventory.printers.isEmpty) {
    return _warningPlan('warn.print.noPrinters', '');
  }
  if (!inventory.contains(params.printer)) {
    return _warningPlan('warn.print.printerUnavailable', params.printer);
  }
  final files = _orderedUniqueFiles(params.files);
  if (files.length > maxFiles) {
    return _warningPlan('warn.print.tooManyFiles', '${files.length}');
  }
  if (params.copies < 1 || params.copies > maxCopies) {
    return _warningPlan('warn.print.invalidCopies', '${params.copies}');
  }
  if (params.retries > maxRetries) {
    return _warningPlan('warn.print.invalidRetries', '${params.retries}');
  }
  if (params.timeoutMinutes < minTimeoutMinutes ||
      params.timeoutMinutes > maxTimeoutMinutes) {
    return _warningPlan(
      'warn.print.invalidTimeout',
      '${params.timeoutMinutes}',
    );
  }

  final normalized = _NormalizedSettings.fromParams(params);
  final rows = <Map<String, Object?>>[];
  final warnings = <Warning>[];
  for (var index = 0; index < files.length; index++) {
    final path = files[index];
    progress(
      ProgressEvent(
        phase: 'print_file_inspection',
        done: index,
        total: files.length,
        subject: '',
      ),
    );
    if (_isExcelWorkbook(path)) {
      final opened = openWorkbook(path);
      final List<Map<String, Object?>> workbookRows;
      try {
        if (opened == null) {
          throw 'warn.print.workbookDiscoveryFailed';
        }
        workbookRows = _rowsFromWorkbook(path, normalized, opened);
      } on String catch (warningKey) {
        warnings.add(_warning(warningKey, _displayName(path)));
        continue;
      }
      if (rows.length + workbookRows.length > maxFiles) {
        return _warningPlan('warn.print.expandedOrderTooLarge', '$maxFiles');
      }
      rows.addAll(workbookRows);
      continue;
    }
    if (!_regularFile(path)) {
      warnings.add(_warning('warn.print.fileMissing', _displayName(path)));
      continue;
    }
    final PrintJobSupport support;
    try {
      support = adapter.inspect(
        path,
        params.printer,
        normalized.printSettings(),
      );
    } on PrintFailure catch (error) {
      if (error.isUnsupported) {
        warnings.add(_warning('warn.print.noHandler', _displayName(path)));
        continue;
      }
      if (error.isUnsupportedOptions) {
        warnings.add(
          _warning('warn.print.unsupportedOptions', _displayName(path)),
        );
        continue;
      }
      throw error.intoBackendError();
    }
    if (support.settingsLimited) {
      warnings.add(_warning('warn.print.settingsLimited', _displayName(path)));
    }
    if (rows.length == maxFiles) {
      return _warningPlan('warn.print.expandedOrderTooLarge', '$maxFiles');
    }
    rows.add(_rowFromFile(path, index, normalized, support));
  }
  progress(
    ProgressEvent(
      phase: 'preview_ready',
      done: files.length,
      total: files.length,
      subject: '',
    ),
  );
  if (rows.length > maxFiles) {
    return _warningPlan('warn.print.expandedOrderTooLarge', '$maxFiles');
  }
  return _planFromRows(
    rows,
    inventory,
    params.collate,
    _uuid.v4(),
    '',
    warnings,
  );
}

Plan revise(
  Plan plan,
  List<Object?> rawRows,
  PrinterInventory inventory,
  PrintAdapter adapter,
) {
  final editor = plan.meta['editor'];
  if (editor is! Map) {
    throw BackendError.validation('print plan has no editor');
  }
  final originals = editor['rows'];
  if (originals is! List) {
    throw BackendError.validation('print plan has no rows');
  }
  final originalById = <String, Map<String, Object?>>{};
  for (final row in originals) {
    if (row is Map) {
      final id = row['_rowId'];
      if (id is String) {
        originalById[id] = Map<String, Object?>.from(row);
      }
    }
  }
  if (rawRows.length != originalById.length) {
    throw BackendError.validation(
      'print queue rows cannot be added or removed',
    );
  }
  final seen = <String>{};
  final rows = <Map<String, Object?>>[];
  for (final rawValue in rawRows) {
    if (rawValue is! Map) {
      throw BackendError.validation('invalid print row');
    }
    final raw = Map<String, Object?>.from(rawValue);
    final rowId = raw['_rowId'] is String ? raw['_rowId'] as String : '';
    if (!seen.add(rowId)) {
      throw BackendError.validation('invalid print row identity');
    }
    final original = originalById[rowId];
    if (original == null) {
      throw BackendError.validation('invalid print row identity');
    }
    for (final field in ['signature', 'worksheet', 'workbookSafety']) {
      if (!_jsonEquals(raw[field], original[field])) {
        throw BackendError.validation('immutable print row metadata changed');
      }
    }
    final printer = _stringField(raw, 'printer');
    if (!inventory.contains(printer)) {
      throw BackendError.validation('printer is unavailable');
    }
    final copies = _unsignedField(raw, 'copies') ?? 0;
    if (copies < 1 || copies > maxCopies) {
      throw BackendError.validation('copies must be between 1 and 99');
    }
    final path = _stringField(original, 'path');
    final parsedPageRange = PageRange.parse(_stringField(raw, 'pageRange'))
        .canonical();
    final settings = PrintSettings(
      paper: _choice(raw, 'paper', _paperChoices, 'A4'),
      orientation: _choice(raw, 'orientation', _orientationChoices, 'auto'),
      scaling: _choice(raw, 'scaling', _scalingChoices, 'fit'),
      color: _choice(raw, 'color', _colorChoices, 'printer'),
      duplex: _choice(raw, 'duplex', _duplexChoices, 'printer'),
      pageRange: parsedPageRange,
      timeoutMinutes: _unsignedField(original, 'timeoutMinutes') ?? 30,
    );
    final worksheetRow = original.containsKey('worksheet');
    final PrintJobSupport support;
    if (worksheetRow) {
      support = PrintJobSupport.controlled();
    } else {
      try {
        support = adapter.inspect(path, printer, settings);
      } on PrintFailure catch (error) {
        throw error.intoBackendError();
      }
    }
    final String pageRange;
    if (worksheetRow) {
      if (_stringField(raw, 'pageRange') != 'all') {
        throw BackendError.validation('error.print.worksheetPageRangeAllOnly');
      }
      pageRange = 'all';
    } else {
      pageRange = canonicalFor(parsedPageRange, support.pageRange);
    }
    final defaultEnabled =
        !original.containsKey('worksheet') ||
        original['worksheetVisibility'] == 'visible';
    final row = Map<String, Object?>.from(original);
    row['enabled'] = raw['enabled'] is bool
        ? raw['enabled'] as bool
        : defaultEnabled;
    row['printer'] = printer;
    row['paper'] = _choice(raw, 'paper', _paperChoices, 'A4');
    row['orientation'] = _choice(
      raw,
      'orientation',
      _orientationChoices,
      'auto',
    );
    row['scaling'] = _choice(raw, 'scaling', _scalingChoices, 'fit');
    row['color'] = _choice(raw, 'color', _colorChoices, 'printer');
    row['duplex'] = _choice(raw, 'duplex', _duplexChoices, 'printer');
    row['copies'] = copies;
    row['verb'] = support.verb;
    row['pageRange'] = pageRange;
    rows.add(row);
  }
  if (originals.any((row) => row is Map && row.containsKey('worksheet')) &&
      !rows.any((row) => row['enabled'] is! bool || row['enabled'] == true)) {
    throw BackendError.validation(
      'print queue requires an enabled worksheet row',
    );
  }
  final warnings = [
    for (final warning in plan.warnings)
      if (warning.key != 'warn.print.nonUndoable' &&
          warning.key != 'warn.print.noJobsSelected')
        warning,
  ];
  return _planFromRows(
    rows,
    inventory,
    plan.meta['collate'] is bool ? plan.meta['collate'] as bool : true,
    _stringMeta(plan, 'queueId'),
    _stringMeta(plan, 'resumeQueueId'),
    warnings,
  );
}

/// Port of `resume_plan`: rebuilds the editor rows from a saved queue.
Plan resumePlan(Map<String, Object?> state, PrinterInventory inventory) {
  final queueId = state['queueId'];
  if (queueId is! String || queueId.isEmpty) {
    throw BackendError.validation('saved print queue is invalid');
  }
  final savedJobs = state['jobs'];
  if (savedJobs is! List) {
    throw BackendError.validation('saved print queue has no jobs');
  }
  final rows = <Map<String, Object?>>[];
  for (var index = 0; index < savedJobs.length; index++) {
    final saved = savedJobs[index];
    if (saved is! Map) {
      throw BackendError.validation('saved print job is invalid');
    }
    final savedMap = Map<String, Object?>.from(saved);
    final status = _stringField(savedMap, 'status');
    final path = _stringField(savedMap, 'path');
    final row = <String, Object?>{};
    row['_rowId'] = 'resume-${index + 1}';
    final jobId = savedMap['jobId'];
    row['_resumeJobId'] = jobId is String ? jobId : 'job-${index + 1}';
    row['enabled'] = const {'pending', 'failed', 'cancelled'}.contains(status);
    row['status'] = status;
    row['file'] = _displayName(path);
    row['path'] = path;
    final format = savedMap['format'];
    row['format'] = format is String && format.isNotEmpty
        ? format
        : _fileFormat(path);
    _copyOrDefault(row, savedMap, 'printer', '');
    _copyOrDefault(row, savedMap, 'paper', 'A4');
    _copyOrDefault(row, savedMap, 'orientation', 'auto');
    _copyOrDefault(row, savedMap, 'scaling', 'fit');
    _copyOrDefault(row, savedMap, 'color', 'printer');
    _copyOrDefault(row, savedMap, 'duplex', 'printer');
    _copyOrDefault(row, savedMap, 'pageRange', 'all');
    row['copies'] = 1;
    _copyOrDefault(row, savedMap, 'retries', 0);
    _copyOrDefault(row, savedMap, 'onError', 'pause');
    _copyOrDefault(row, savedMap, 'timeoutMinutes', 30);
    _copyOrDefault(row, savedMap, 'verb', '');
    _copyOrDefault(row, savedMap, 'signature', <String, Object?>{});
    final worksheet = savedMap['worksheet'];
    if (worksheet is Map) {
      row['worksheet'] = worksheet;
      row['worksheetName'] = worksheet['name'];
      row['worksheetVisibility'] = worksheet['visibility'];
    }
    if (savedMap.containsKey('workbookSafety')) {
      row['workbookSafety'] = savedMap['workbookSafety'];
    }
    rows.add(row);
  }
  return _planFromRows(rows, inventory, false, queueId, queueId, [
    _warning('warn.print.resumingQueue', _takeChars(queueId, 8)),
  ]);
}

/// Port of `validate_job`.
void validateJob(PrintJob job) {
  if (job.jobId.isEmpty ||
      job.path.isEmpty ||
      job.printer.isEmpty ||
      !const {'docflow', 'printto', 'print'}.contains(job.verb) ||
      !_paperChoices.contains(job.paper) ||
      !_orientationChoices.contains(job.orientation) ||
      !_scalingChoices.contains(job.scaling) ||
      !_colorChoices.contains(job.color) ||
      !_duplexChoices.contains(job.duplex) ||
      !_errorActionChoices.contains(job.onError) ||
      job.retries > maxRetries ||
      job.timeoutMinutes < minTimeoutMinutes ||
      job.timeoutMinutes > maxTimeoutMinutes ||
      job.copy == 0) {
    throw BackendError.validation('print plan contains an invalid job');
  }
  final worksheet = job.worksheet;
  if (worksheet != null) {
    final safety = job.workbookSafety;
    if (!const {'XLSX', 'XLSM'}.contains(job.format) ||
        job.verb != 'docflow' ||
        job.pageRange != 'all' ||
        !job.signature.hasValidSha256 ||
        safety == null ||
        workbookSafetyIsUnsafe(safety) ||
        worksheet.workbookIndex == 0 ||
        worksheet.sheetId == 0 ||
        worksheet.relationshipId.isEmpty ||
        worksheet.partPath.isEmpty ||
        worksheet.name.isEmpty) {
      throw BackendError.validation(
        job.pageRange != 'all'
            ? 'error.print.worksheetPageRangeAllOnly'
            : 'print plan contains invalid worksheet metadata',
      );
    }
  } else if (job.workbookSafety != null) {
    throw BackendError.validation(
      'print plan contains invalid worksheet metadata',
    );
  }
  final canonical = canonicalFor(job.pageRange, _jobPageRangeCapability(job));
  if (canonical != job.pageRange) {
    throw BackendError.validation('error.print.pageRangeInvalid');
  }
}

PageRangeCapability _jobPageRangeCapability(PrintJob job) {
  if (job.worksheet != null) {
    return const AllOnlyPageRangeCapability();
  }
  if (job.verb == 'printto' || job.verb == 'print') {
    return const AllOnlyPageRangeCapability();
  }
  return switch (job.format) {
    'PDF' => const PdfPageRangeCapability(),
    'PNG' ||
    'JPG' ||
    'JPEG' ||
    'BMP' ||
    'TIF' ||
    'TIFF' => const SinglePagePageRangeCapability(),
    _ => const AllOnlyPageRangeCapability(),
  };
}

Plan _planFromRows(
  List<Map<String, Object?>> rows,
  PrinterInventory inventory,
  bool collate,
  String queueId,
  String resumeQueueId,
  List<Warning> warningsIn,
) {
  final jobs = _jobsFromRows(rows, collate);
  final warnings = List<Warning>.of(warningsIn);
  final steps = <Step>[
    for (final row in rows)
      if (row['enabled'] is! bool || row['enabled'] == true)
        () {
          final path = _stringField(row, 'path');
          final meta = <String, Object?>{};
          for (final key in const [
            'printer',
            'verb',
            'format',
            'copies',
            'paper',
            'orientation',
            'scaling',
            'color',
            'duplex',
            'pageRange',
          ]) {
            if (row.containsKey(key)) {
              meta[key] = row[key];
            }
          }
          return Step(
            kind: StepKind.print,
            src: path,
            dst: path,
            noteKey: 'note.print.queued',
            meta: meta,
          );
        }(),
  ];
  if (steps.isNotEmpty &&
      !warnings.any((warning) => warning.key == 'warn.print.nonUndoable')) {
    warnings.add(_warning('warn.print.nonUndoable', ''));
  }
  if (steps.isEmpty) {
    warnings.add(_warning('warn.print.noJobsSelected', ''));
  }
  return Plan(
    planId: '',
    opId: 'files.batch_print',
    steps: steps,
    warnings: warnings,
    meta: {
      'queueId': queueId,
      'resumeQueueId': resumeQueueId,
      'collate': collate,
      'jobs': [for (final job in jobs) job.toJson()],
      'fileCount': steps.length,
      'jobCount': jobs.length,
      'nonUndoable': true,
      'editor': _editor(rows, inventory.printers),
    },
  );
}

List<PrintJob> _jobsFromRows(List<Map<String, Object?>> rows, bool collate) {
  final selected = [
    for (final row in rows)
      if (row['enabled'] is! bool || row['enabled'] == true) row,
  ];
  final jobs = <PrintJob>[];
  if (collate) {
    var maxCopies = 0;
    for (final row in selected) {
      final copies = _unsignedField(row, 'copies') ?? 0;
      if (copies > maxCopies) {
        maxCopies = copies;
      }
    }
    for (var copy = 1; copy <= maxCopies; copy++) {
      for (final row in selected) {
        if ((_unsignedField(row, 'copies') ?? 0) >= copy) {
          jobs.add(_jobFromRow(row, copy));
        }
      }
    }
  } else {
    for (final row in selected) {
      final copies = _unsignedField(row, 'copies') ?? 0;
      for (var copy = 1; copy <= copies; copy++) {
        jobs.add(_jobFromRow(row, copy));
      }
    }
  }
  return jobs;
}

PrintJob _jobFromRow(Map<String, Object?> row, int copy) {
  final rowId = row['_rowId'] is String
      ? row['_rowId'] as String
      : _uuid.v4().replaceAll('-', '');
  final resumeJobId = row['_resumeJobId'];
  final jobId = resumeJobId is String ? resumeJobId : '$rowId-c$copy';
  return PrintJob(
    jobId: jobId,
    path: _stringField(row, 'path'),
    printer: _stringField(row, 'printer'),
    verb: _stringField(row, 'verb'),
    format: _stringField(row, 'format'),
    paper: _stringField(row, 'paper'),
    orientation: _stringField(row, 'orientation'),
    scaling: _stringField(row, 'scaling'),
    color: _stringField(row, 'color'),
    duplex: _stringField(row, 'duplex'),
    pageRange: _stringField(row, 'pageRange'),
    timeoutMinutes: _unsignedField(row, 'timeoutMinutes') ?? 30,
    retries: _unsignedField(row, 'retries') ?? 0,
    onError: _stringField(row, 'onError'),
    copy: copy,
    signature: _jsonFieldOrNull(row, 'signature', FileSignature.fromJson),
    worksheet: _jsonFieldOrNull(row, 'worksheet', worksheetDescriptorFromJson),
    workbookSafety: _jsonFieldOrNull(
      row,
      'workbookSafety',
      workbookSafetyFromJson,
    ),
  );
}

T? _jsonFieldOrNull<T>(
  Map<String, Object?> row,
  String key,
  T Function(Object?) parse,
) {
  final value = row[key];
  if (value == null) {
    return null;
  }
  try {
    return parse(value);
  } on Object {
    return null;
  }
}

Map<String, Object?> _rowFromFile(
  String path,
  int index,
  _NormalizedSettings settings,
  PrintJobSupport support,
) {
  final file = File(path);
  final FileSignature signature;
  try {
    signature = fileSignatureFromFile(file, includeSha256: false);
  } on FileSystemException {
    throw BackendError.pathInvalid('print file is unavailable');
  }
  return {
    '_rowId': 'file-${index + 1}',
    'enabled': true,
    'status': 'ready',
    'file': _displayName(path),
    'path': path,
    'format': _fileFormat(path),
    'printer': settings.printer,
    'paper': settings.paper,
    'orientation': settings.orientation,
    'scaling': settings.scaling,
    'color': settings.color,
    'duplex': settings.duplex,
    'pageRange': 'all',
    'copies': settings.copies,
    'retries': settings.retries,
    'onError': settings.onError,
    'timeoutMinutes': settings.timeoutMinutes,
    'verb': support.verb,
    'signature': signature.toJson(),
  };
}

List<Map<String, Object?>> _rowsFromWorkbook(
  String path,
  _NormalizedSettings settings,
  OpenedPrintFile file,
) {
  final FileSignature signature;
  final OrderedWorkbookDiscovery discovery;
  try {
    signature = fileSignatureFromOpenFile(file, includeSha256: true);
    discovery = orderedWorkbookDiscovery(file.readAllFromStart());
  } on OoxmlError catch (error) {
    if (error.message == 'workbook has too many worksheets') {
      throw 'warn.print.workbookTooManyWorksheets';
    }
    throw 'warn.print.workbookDiscoveryFailed';
  } on BackendError {
    throw 'warn.print.workbookDiscoveryFailed';
  }
  final safetyKey = _workbookSafetyWarning(discovery.safety);
  if (safetyKey != null) {
    throw safetyKey;
  }
  return [
    for (final worksheet in discovery.worksheets)
      _rowFromWorksheet(path, settings, signature, discovery.safety, worksheet),
  ];
}

Map<String, Object?> _rowFromWorksheet(
  String path,
  _NormalizedSettings settings,
  FileSignature signature,
  WorkbookSafetyFacts safety,
  WorksheetDescriptor worksheet,
) => {
  '_rowId': _uuid.v4(),
  'enabled': worksheet.visibility == WorksheetVisibility.visible,
  'status': 'ready',
  'file': _displayName(path),
  'path': path,
  'format': _fileFormat(path),
  'worksheetName': worksheet.name,
  'worksheetVisibility': worksheet.visibility.name,
  'worksheet': worksheetDescriptorToJson(worksheet),
  'signature': signature.toJson(),
  'workbookSafety': workbookSafetyToJson(safety),
  'printer': settings.printer,
  'paper': settings.paper,
  'orientation': settings.orientation,
  'scaling': settings.scaling,
  'color': settings.color,
  'duplex': settings.duplex,
  'pageRange': 'all',
  'copies': settings.copies,
  'retries': settings.retries,
  'onError': settings.onError,
  'timeoutMinutes': settings.timeoutMinutes,
  'verb': 'docflow',
};

class _NormalizedSettings {
  const _NormalizedSettings({
    required this.printer,
    required this.paper,
    required this.orientation,
    required this.scaling,
    required this.color,
    required this.duplex,
    required this.copies,
    required this.retries,
    required this.onError,
    required this.timeoutMinutes,
  });

  factory _NormalizedSettings.fromParams(BatchPrintParams params) =>
      _NormalizedSettings(
        printer: params.printer,
        paper: _normalizedChoice(params.paper, _paperChoices, 'A4'),
        orientation: _normalizedChoice(
          params.orientation,
          _orientationChoices,
          'auto',
        ),
        scaling: _normalizedChoice(params.scaling, _scalingChoices, 'fit'),
        color: _normalizedChoice(params.color, _colorChoices, 'printer'),
        duplex: _normalizedChoice(params.duplex, _duplexChoices, 'printer'),
        copies: params.copies,
        retries: params.retries,
        onError: _normalizedChoice(
          params.onError,
          _errorActionChoices,
          'pause',
        ),
        timeoutMinutes: params.timeoutMinutes,
      );

  final String printer;
  final String paper;
  final String orientation;
  final String scaling;
  final String color;
  final String duplex;
  final int copies;
  final int retries;
  final String onError;
  final int timeoutMinutes;

  PrintSettings printSettings() => PrintSettings(
    paper: paper,
    orientation: orientation,
    scaling: scaling,
    color: color,
    duplex: duplex,
    pageRange: 'all',
    timeoutMinutes: timeoutMinutes,
  );
}

Map<String, Object?> _editor(
  List<Map<String, Object?>> rows,
  List<String> printers,
) {
  final reorderable = rows.any((row) => row.containsKey('worksheet'));
  final columns = <Map<String, Object?>>[
    {
      'key': 'enabled',
      'labelKey': 'print.col.include',
      'kind': 'bool',
      'width': 64,
    },
    {
      'key': 'status',
      'labelKey': 'print.col.status',
      'kind': 'readonly',
      'width': 90,
      'bulk': false,
    },
    {
      'key': 'file',
      'labelKey': 'print.col.file',
      'kind': 'readonly',
      'width': 260,
      'bulk': false,
    },
    {
      'key': 'format',
      'labelKey': 'print.col.format',
      'kind': 'readonly',
      'width': 76,
      'bulk': false,
    },
  ];
  if (reorderable) {
    columns.add({
      'key': 'worksheetName',
      'labelKey': 'print.col.worksheet',
      'kind': 'readonly',
      'width': 180,
      'bulk': false,
    });
    columns.add({
      'key': 'worksheetVisibility',
      'labelKey': 'print.col.worksheetVisibility',
      'kind': 'readonly',
      'width': 112,
      'bulk': false,
    });
  }
  columns.addAll([
    {
      'key': 'pageRange',
      'labelKey': 'print.col.pageRange',
      'helpKey': 'print.col.pageRange.help',
      'kind': 'text',
      'width': 112,
      'bulk': false,
    },
    {
      'key': 'printer',
      'labelKey': 'print.col.printer',
      'kind': 'choice',
      'width': 190,
    },
    {
      'key': 'paper',
      'labelKey': 'print.col.paper',
      'kind': 'choice',
      'width': 88,
    },
    {
      'key': 'orientation',
      'labelKey': 'print.col.orientation',
      'kind': 'choice',
      'width': 112,
    },
    {
      'key': 'scaling',
      'labelKey': 'print.col.scaling',
      'kind': 'choice',
      'width': 104,
    },
    {
      'key': 'color',
      'labelKey': 'print.col.color',
      'kind': 'choice',
      'width': 110,
    },
    {
      'key': 'duplex',
      'labelKey': 'print.col.duplex',
      'kind': 'choice',
      'width': 116,
    },
    {
      'key': 'copies',
      'labelKey': 'print.col.copies',
      'kind': 'int',
      'width': 76,
    },
  ]);
  return {
    'kind': 'recordTable',
    'layout': 'compact',
    'titleKey': 'print.editor.title',
    'rowCountKey': 'print.editor.rowCount',
    'groupField': 'format',
    'selectionGroupFields': ['format'],
    'columns': columns,
    'rows': rows,
    'choices': {
      'printer': printers,
      'paper': _paperChoices,
      'orientation': _orientationChoices,
      'scaling': _scalingChoices,
      'color': _colorChoices,
      'duplex': _duplexChoices,
    },
    'choiceLabels': {
      'paper': _choiceLabels('choice.print.paper', _paperChoices),
      'orientation': _choiceLabels(
        'choice.print.orientation',
        _orientationChoices,
      ),
      'scaling': _choiceLabels('choice.print.scaling', _scalingChoices),
      'color': _choiceLabels('choice.print.color', _colorChoices),
      'duplex': _choiceLabels('choice.print.duplex', _duplexChoices),
    },
    'bulkEnabled': true,
    'selectionEnabled': true,
    'duplicateEnabled': false,
    'reorderable': reorderable,
  };
}

String? _workbookSafetyWarning(WorkbookSafetyFacts safety) {
  if (safety.hasXlmMacroSheet) {
    return 'warn.print.workbookUnsafeXlm';
  }
  if (safety.hasWorkbookProtection) {
    return 'warn.print.workbookUnsafeProtection';
  }
  if (safety.externalLinkCount > 0) {
    return 'warn.print.workbookUnsafeExternalLinks';
  }
  if (safety.connectionCount > 0) {
    return 'warn.print.workbookUnsafeConnections';
  }
  if (safety.queryTableCount > 0) {
    return 'warn.print.workbookUnsafeQueryTables';
  }
  return null;
}

Map<String, String> _choiceLabels(String prefix, List<String> choices) => {
  for (final choice in choices) choice: '$prefix.$choice',
};

List<String> _orderedUniqueFiles(List<String> raw) {
  final seen = <String>{};
  final out = <String>[];
  for (final path in raw) {
    final absolute = _absolutePath(path);
    final key = Platform.isWindows ? absolute.toLowerCase() : absolute;
    if (seen.add(key)) {
      out.add(absolute);
    }
  }
  return out;
}

String _absolutePath(String path) =>
    p.isAbsolute(path) ? path : p.join(Directory.current.path, path);

bool _regularFile(String path) {
  final type = FileSystemEntity.typeSync(path, followLinks: false);
  return type == FileSystemEntityType.file;
}

String _fileFormat(String path) {
  final extension = p.extension(path);
  final name = extension.startsWith('.') ? extension.substring(1) : extension;
  return name.isEmpty ? 'FILE' : name.toUpperCase();
}

bool _isExcelWorkbook(String path) {
  final extension = p.extension(path).toLowerCase();
  return extension == '.xlsx' || extension == '.xlsm';
}

String _displayName(String path) => p.basename(path);

Plan _warningPlan(String key, String subject) => Plan(
  planId: '',
  opId: 'files.batch_print',
  steps: const [],
  warnings: [_warning(key, subject)],
  meta: const {},
);

Warning _warning(String key, String subject) =>
    Warning(key: key, subject: subject);

String _normalizedChoice(String value, List<String> choices, String fallback) =>
    choices.contains(value) ? value : fallback;

String _choice(
  Map<String, Object?> row,
  String key,
  List<String> choices,
  String fallback,
) => _normalizedChoice(_stringField(row, key), choices, fallback);

String _stringField(Map<String, Object?> row, String key) {
  final value = row[key];
  return value is String ? value : '';
}

int? _unsignedField(Map<String, Object?> row, String key) {
  final value = row[key];
  return value is int && value >= 0 ? value : null;
}

String _stringMeta(Plan plan, String key) {
  final value = plan.meta[key];
  return value is String ? value : '';
}

void _copyOrDefault(
  Map<String, Object?> target,
  Map<String, Object?> source,
  String key,
  Object? fallback,
) {
  target[key] = source.containsKey(key) ? source[key] : fallback;
}

String _takeChars(String value, int count) =>
    String.fromCharCodes(value.runes.take(count));

bool _jsonEquals(Object? a, Object? b) {
  if (a is Map && b is Map) {
    if (a.length != b.length) {
      return false;
    }
    for (final key in a.keys) {
      if (!b.containsKey(key) || !_jsonEquals(a[key], b[key])) {
        return false;
      }
    }
    return true;
  }
  if (a is List && b is List) {
    if (a.length != b.length) {
      return false;
    }
    for (var i = 0; i < a.length; i++) {
      if (!_jsonEquals(a[i], b[i])) {
        return false;
      }
    }
    return true;
  }
  return a == b;
}
