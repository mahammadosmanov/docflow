/// Port of src-tauri/src/rfi_system_sync.rs: RFI-system scanning and exact,
/// guarded workbook synchronization.
///
/// Dart-specific notes:
/// - Rust takes open `File` handles for the workbooks. Here the caller passes
///   a [FollowUpWorkbookFile] (path, bytes, identity, mtime) captured from one
///   open handle; [openSyncWorkbookFile] builds it exactly the way
///   `FilesystemAuthority` fingerprints workbooks at apply time.
/// - Directory discovery runs in worker isolates through [boundedMap] /
///   [boundedMapStream]. Worley signature analysis needs the asynchronous PDF
///   page renderer and the scan index database, neither of which can cross an
///   isolate boundary, so it runs on the calling isolate with up to
///   [workerCount] concurrent analyses, reporting completions in completion
///   order exactly like `bounded_map_stream`.
/// - UNVERIFIED: page rendering uses [selectPdfPageRenderer] by default, whose
///   PDFKit / Windows PDF bridges are not ported yet (ocr.dart). Until they
///   are, callers on macOS/Windows must inject a [PdfPageRenderer]; otherwise
///   every analysis reports a transient `PDF render error:` outcome.
library;

import 'dart:collection';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:path/path.dart' as p;
import 'package:unorm_dart/unorm_dart.dart' as unorm;
import 'package:uuid/uuid.dart';

import 'cap_fs.dart' show OsHandle, fileIdentity, openSourceNoFollow;
import 'contracts.dart';
import 'errors.dart';
import 'fs_ops.dart'
    show
        RfiSystemSyncPort,
        RfiSystemSyncStepMeta,
        fileSha256,
        rfiCompleteStatusSyncOperationId,
        rfiSystemSyncOperationId;
import 'journal.dart'
    show FilesystemObjectIdentity, UnixObjectIdentity, WindowsObjectIdentity;
import 'naming.dart' show filePrefix;
import 'ocr.dart'
    show
        GrayImage,
        PdfPageRenderer,
        readPngGray,
        renderFirstPagePng,
        selectPdfPageRenderer;
import 'ooxml.dart';
import 'operations/process_scans.dart' show OcrFailure;
import 'parallel.dart';
import 'rfi_code.dart' show normalizeCandidateNumber;
import 'rfi_follow_up.dart'
    show
        FollowUpFileIdentity,
        FollowUpFingerprint,
        FollowUpWorkbookFile,
        asciiUpper,
        fingerprintWorkbookFile;
import 'scan_index.dart';

export 'rfi_follow_up.dart' show FollowUpWorkbookFile;

/// `OPERATION_ID`.
const String operationId = rfiSystemSyncOperationId;

/// `COMPLETE_STATUS_OPERATION_ID`.
const String completeStatusOperationId = rfiCompleteStatusSyncOperationId;

/// `WORLEY_SIGNED_MARKER`.
const String worleySignedMarker = 'Signed by Worley';

/// `DISCIPLINES`.
const List<String> disciplines = ['CIV', 'MEC', 'ELE', 'PRC'];

const int _maxHeaderRows = 32;
const int _maxHeaderColumns = 64;
const String _systemScanProfile = 'worley-first-page-v2-others-only';
const int _maxCompletionEntries = 250000;
const int _maxCompletionWarnings = 100;
const Uuid _uuid = Uuid();

// ---------------------------------------------------------------------------
// Records and metadata
// ---------------------------------------------------------------------------

/// Port of `ScannedRfiRecord` (serde camelCase).
class ScannedRfiRecord {
  ScannedRfiRecord({
    required this.discipline,
    required this.number,
    required this.canonicalRfi,
    required this.shortRfi,
    required this.rfiFolder,
    required this.pdfPath,
    required this.signedByWorley,
    required this.signatureReason,
  });

  final String discipline;
  final int number;
  final String canonicalRfi;
  final String shortRfi;
  final String rfiFolder;
  final String? pdfPath;
  bool signedByWorley;
  String signatureReason;

  ScannedRfiRecord copy() => ScannedRfiRecord(
    discipline: discipline,
    number: number,
    canonicalRfi: canonicalRfi,
    shortRfi: shortRfi,
    rfiFolder: rfiFolder,
    pdfPath: pdfPath,
    signedByWorley: signedByWorley,
    signatureReason: signatureReason,
  );

  JsonObject toJson() => {
    'discipline': discipline,
    'number': number,
    'canonicalRfi': canonicalRfi,
    'shortRfi': shortRfi,
    'rfiFolder': rfiFolder,
    'pdfPath': pdfPath,
    'signedByWorley': signedByWorley,
    'signatureReason': signatureReason,
  };

  @override
  bool operator ==(Object other) =>
      other is ScannedRfiRecord &&
      other.discipline == discipline &&
      other.number == number &&
      other.canonicalRfi == canonicalRfi &&
      other.shortRfi == shortRfi &&
      other.rfiFolder == rfiFolder &&
      other.pdfPath == pdfPath &&
      other.signedByWorley == signedByWorley &&
      other.signatureReason == signatureReason;

  @override
  int get hashCode => Object.hash(
    discipline,
    number,
    canonicalRfi,
    shortRfi,
    rfiFolder,
    pdfPath,
    signedByWorley,
    signatureReason,
  );

  @override
  String toString() => 'ScannedRfiRecord(${jsonEncode(toJson())})';
}

/// Port of `CachedWorleyDetection` (camelCase, deny_unknown_fields).
class _CachedWorleyDetection {
  const _CachedWorleyDetection({
    required this.signedByWorley,
    required this.signatureReason,
    required this.sourceSha256,
  });

  final bool signedByWorley;
  final String signatureReason;
  final String? sourceSha256;

  JsonObject toJson() => {
    'signedByWorley': signedByWorley,
    'signatureReason': signatureReason,
    'sourceSha256': sourceSha256,
  };

  static _CachedWorleyDetection fromJson(Object? value) {
    if (value is! Map) {
      throw const FormatException('cached detection must be an object');
    }
    for (final key in value.keys) {
      if (key is! String ||
          !const {
            'signedByWorley',
            'signatureReason',
            'sourceSha256',
          }.contains(key)) {
        throw FormatException('unknown field `$key`');
      }
    }
    final signed = value['signedByWorley'];
    final reason = value['signatureReason'];
    final sha = value['sourceSha256'];
    if (signed is! bool) {
      throw const FormatException('missing field `signedByWorley`');
    }
    if (reason is! String) {
      throw const FormatException('missing field `signatureReason`');
    }
    if (sha != null && sha is! String) {
      throw const FormatException('invalid type for `sourceSha256`');
    }
    return _CachedWorleyDetection(
      signedByWorley: signed,
      signatureReason: reason,
      sourceSha256: sha as String?,
    );
  }
}

class _CandidateAnalysis {
  const _CandidateAnalysis({
    required this.record,
    required this.relative,
    required this.fingerprint,
    required this.detection,
    required this.reused,
    required this.transientFailure,
    required this.subject,
  });

  final ScannedRfiRecord record;
  final String? relative;
  final FileFingerprint? fingerprint;
  final _CachedWorleyDetection? detection;
  final bool reused;
  final bool transientFailure;
  final String subject;
}

class _CandidateAnalysisError implements Exception {
  const _CandidateAnalysisError(this.relative, this.message);

  final String? relative;
  final String message;

  @override
  String toString() => message;
}

/// Port of `SyncWorkbookKind` (camelCase).
enum SyncWorkbookKind {
  rfiLog('rfiLog'),
  rfiLogStatus('rfiLogStatus'),
  followUp('followUp');

  const SyncWorkbookKind(this.wire);

  final String wire;
}

/// Port of `GuardedCellUpdateDto` (camelCase, deny_unknown_fields).
class GuardedCellUpdateDto {
  const GuardedCellUpdateDto({
    required this.rowId,
    required this.expectedValue,
    required this.expectedFormula,
    required this.expectedStyle,
    required this.replacementValue,
    required this.row,
    required this.col,
  });

  final String rowId;
  final String expectedValue;
  final String? expectedFormula;
  final String? expectedStyle;
  final String replacementValue;
  final int row;
  final int col;

  JsonObject toJson() => {
    'rowId': rowId,
    'expectedValue': expectedValue,
    'expectedFormula': expectedFormula,
    'expectedStyle': expectedStyle,
    'replacementValue': replacementValue,
    'row': row,
    'col': col,
  };

  factory GuardedCellUpdateDto.fromJson(Object? value) {
    final map = _strictObject(
      value,
      const {
        'rowId',
        'expectedValue',
        'expectedFormula',
        'expectedStyle',
        'replacementValue',
        'row',
        'col',
      },
      const {'rowId', 'expectedValue', 'replacementValue', 'row', 'col'},
    );
    return GuardedCellUpdateDto(
      rowId: _requiredString(map, 'rowId'),
      expectedValue: _requiredString(map, 'expectedValue'),
      expectedFormula: _optionalString(map, 'expectedFormula'),
      expectedStyle: _optionalString(map, 'expectedStyle'),
      replacementValue: _requiredString(map, 'replacementValue'),
      row: _requiredU32(map, 'row'),
      col: _requiredU32(map, 'col'),
    );
  }
}

/// Port of `SystemSyncStepMeta` (camelCase, deny_unknown_fields). Extends the
/// fs_ops view so `FilesystemAuthority` can read the apply-time fields.
class SystemSyncStepMeta extends RfiSystemSyncStepMeta {
  SystemSyncStepMeta({
    required super.targetPath,
    required super.backupPath,
    required super.sourceFingerprint,
    required this.workbookKind,
    required this.updates,
    required this.canonicalRfiCodes,
    required this.summary,
  });

  final SyncWorkbookKind workbookKind;

  /// Sheet -> "row:col" -> update, both levels in Rust `BTreeMap` order.
  final SplayTreeMap<String, SplayTreeMap<String, GuardedCellUpdateDto>>
  updates;
  List<String> canonicalRfiCodes;
  final String summary;

  JsonObject toJson() => {
    'targetPath': targetPath,
    'backupPath': backupPath,
    'sourceFingerprint': sourceFingerprint.toJson(),
    'workbookKind': workbookKind.wire,
    'updates': {
      for (final MapEntry(key: sheet, value: cells) in updates.entries)
        sheet: {
          for (final MapEntry(key: key, value: update) in cells.entries)
            key: update.toJson(),
        },
    },
    'canonicalRfiCodes': List<String>.of(canonicalRfiCodes),
    'summary': summary,
  };

  factory SystemSyncStepMeta.fromJson(Object? value) {
    final map = _strictObject(
      value,
      const {
        'targetPath',
        'backupPath',
        'sourceFingerprint',
        'workbookKind',
        'updates',
        'canonicalRfiCodes',
        'summary',
      },
      const {
        'targetPath',
        'backupPath',
        'sourceFingerprint',
        'workbookKind',
        'updates',
        'summary',
      },
    );
    final kindWire = map['workbookKind'];
    final kind = SyncWorkbookKind.values
        .where((kind) => kind.wire == kindWire)
        .firstOrNull;
    if (kind == null) {
      throw FormatException('unknown variant `$kindWire`');
    }
    final rawUpdates = map['updates'];
    if (rawUpdates is! Map) {
      throw const FormatException('invalid type for `updates`');
    }
    final updates = _updatesMap();
    for (final MapEntry(key: sheet, value: cells) in rawUpdates.entries) {
      if (sheet is! String || cells is! Map) {
        throw const FormatException('invalid type for `updates`');
      }
      final sheetUpdates = _cellUpdatesMap();
      for (final MapEntry(key: key, value: update) in cells.entries) {
        if (key is! String) {
          throw const FormatException('invalid type for `updates`');
        }
        sheetUpdates[key] = GuardedCellUpdateDto.fromJson(update);
      }
      updates[sheet] = sheetUpdates;
    }
    final codes = <String>[];
    final rawCodes = map['canonicalRfiCodes'];
    if (rawCodes != null) {
      if (rawCodes is! List) {
        throw const FormatException('invalid type for `canonicalRfiCodes`');
      }
      for (final code in rawCodes) {
        if (code is! String) {
          throw const FormatException('invalid type for `canonicalRfiCodes`');
        }
        codes.add(code);
      }
    } else if (map.containsKey('canonicalRfiCodes')) {
      throw const FormatException('invalid type for `canonicalRfiCodes`');
    }
    return SystemSyncStepMeta(
      targetPath: _requiredString(map, 'targetPath'),
      backupPath: _requiredString(map, 'backupPath'),
      sourceFingerprint: FollowUpFingerprint.fromJson(map['sourceFingerprint']),
      workbookKind: kind,
      updates: updates,
      canonicalRfiCodes: codes,
      summary: _requiredString(map, 'summary'),
    );
  }
}

class _RfiLogRowInfo {
  const _RfiLogRowInfo({
    required this.sheet,
    required this.row,
    required this.canonicalRfi,
    required this.remarksCol,
    required this.remarksCell,
    required this.remarksMerged,
  });

  final String sheet;
  final int row;
  final String canonicalRfi;
  final int remarksCol;
  final CellData? remarksCell;
  final bool remarksMerged;
}

class _RfiLogStatusRowInfo {
  const _RfiLogStatusRowInfo({
    required this.sheet,
    required this.row,
    required this.canonicalRfi,
    required this.statusCol,
    required this.statusCell,
    required this.statusMerged,
  });

  final String sheet;
  final int row;
  final String canonicalRfi;
  final int statusCol;
  final CellData? statusCell;
  final bool statusMerged;
}

class _CompletionPdfDiscovery {
  const _CompletionPdfDiscovery({
    required this.matches,
    required this.invalidNames,
    required this.workerCount,
    required this.directoriesScanned,
    required this.entriesScanned,
  });

  final SplayTreeMap<String, List<String>> matches;
  final List<String> invalidNames;
  final int workerCount;
  final int directoriesScanned;
  final int entriesScanned;
}

class _CompletionDirectoryBatch {
  _CompletionDirectoryBatch();

  final List<String> directories = [];
  final List<(String, String)> matches = [];
  final List<String> invalidNames = [];
  int entriesScanned = 0;
}

/// `Result<CompletionDirectoryBatch, BackendError>` across an isolate.
class _CompletionDirectoryResult {
  const _CompletionDirectoryResult(this.batch, this.error);

  final _CompletionDirectoryBatch? batch;
  final BackendError? error;
}

/// `Result<Vec<(ScannedRfiRecord, PathBuf)>, String>` across an isolate.
class _DisciplineResult {
  const _DisciplineResult(this.candidates, this.error);

  final List<(ScannedRfiRecord, String)>? candidates;
  final String? error;
}

class _FollowUpTarget {
  const _FollowUpTarget({
    required this.sheet,
    required this.row,
    required this.statusCol,
    required this.statusCell,
    required this.statusMerged,
  });

  final String sheet;
  final int row;
  final int statusCol;
  final CellData? statusCell;
  final bool statusMerged;
}

class _HeaderLayout {
  const _HeaderLayout(this.row, this.rfiCol, this.valueCol);

  final int row;
  final int rfiCol;
  final int valueCol;
}

// ---------------------------------------------------------------------------
// Identity helpers
// ---------------------------------------------------------------------------

/// `canonical_rfi_prefix`.
String canonicalRfiPrefix() => 'AZE-SUN-INT-QMS-RFI-';

final RegExp _rfiNamePattern = RegExp(
  r'^(?:AZE-SUN-INT-QMS-RFI-|RFI-)?(CIV|MEC|ELE|PRC)[-_ ]*([0-9]{1,4})$',
);

/// `parse_rfi_discipline_and_number`.
(String, int)? parseRfiDisciplineAndNumber(String name) {
  final clean = asciiUpper(_rsTrim(unorm.nfc(name)));
  final captures = _rfiNamePattern.firstMatch(clean);
  if (captures == null) {
    return null;
  }
  final discipline = switch (captures.group(1)) {
    'CIV' => 'CIV',
    'MEC' => 'MEC',
    'ELE' => 'ELE',
    'PRC' => 'PRC',
    _ => null,
  };
  if (discipline == null) {
    return null;
  }
  final number = normalizeCandidateNumber(captures.group(2)!);
  if (number == null) {
    return null;
  }
  return (discipline, int.parse(number));
}

String _pad4(int number) => number.toString().padLeft(4, '0');

/// `exact_canonical_rfi`.
String? exactCanonicalRfi(String value) {
  final clean = asciiUpper(_rsTrim(unorm.nfc(value)));
  final parsed = parseRfiDisciplineAndNumber(clean);
  if (parsed == null) {
    return null;
  }
  final (discipline, number) = parsed;
  return '${canonicalRfiPrefix()}$discipline-${_pad4(number)}';
}

/// `completion_code_from_filename`.
String? _completionCodeFromFilename(String name) {
  final lower = _asciiLower(name);
  final prefix = _asciiLower(filePrefix);
  if (!lower.startsWith(prefix) || !lower.endsWith('.pdf')) {
    return null;
  }
  final end = name.length - 4;
  if (end < filePrefix.length) {
    return null;
  }
  return exactCanonicalRfi(name.substring(filePrefix.length, end));
}

// ---------------------------------------------------------------------------
// Completion PDF discovery
// ---------------------------------------------------------------------------

Future<_CompletionPdfDiscovery> _discoverCompletionPdfs(
  String rootPath,
  void Function(ProgressEvent) progress,
  bool Function() cancelled,
) async {
  final String root;
  try {
    root = _canonicalize(rootPath);
  } on Object {
    throw BackendError.pathInvalid('RFI system root could not be resolved');
  }
  if (!FileSystemEntity.isDirectorySync(root)) {
    throw BackendError.pathInvalid('RFI system root is not a directory');
  }
  var directories = <String>[root];
  final matches = SplayTreeMap<String, List<String>>(_compareUtf8);
  final invalidNames = <String>[];
  var workerCount = 1;
  var directoriesScanned = 0;
  var entriesScanned = 0;
  progress(
    ProgressEvent(
      phase: 'completion_pdf_discovery',
      done: 0,
      total: 1,
      subject: root,
    ),
  );
  while (directories.isNotEmpty) {
    if (cancelled()) {
      throw BackendError.cancelled();
    }
    directories.sort(_comparePaths);
    final currentCount = directories.length;
    final int workers;
    final List<_CompletionDirectoryResult> batches;
    try {
      (
        workers,
        batches,
      ) = await boundedMap<(String, String), _CompletionDirectoryResult>([
        for (final directory in directories) (root, directory),
      ], _scanCompletionDirectoryTask);
    } on Object catch (error) {
      throw BackendError.requestFailed('$error');
    }
    if (workers > workerCount) {
      workerCount = workers;
    }
    final next = <String>[];
    for (final result in batches) {
      final error = result.error;
      if (error != null) {
        throw error;
      }
      final batch = result.batch!;
      entriesScanned += batch.entriesScanned;
      if (entriesScanned > _maxCompletionEntries) {
        throw BackendError.validation(
          'RFI system exceeds the $_maxCompletionEntries-entry completion scan limit',
        );
      }
      next.addAll(batch.directories);
      invalidNames.addAll(batch.invalidNames);
      for (final (code, path) in batch.matches) {
        matches.putIfAbsent(code, () => <String>[]).add(path);
      }
    }
    directoriesScanned += currentCount;
    next.sort(_comparePaths);
    progress(
      ProgressEvent(
        phase: 'completion_pdf_discovery',
        done: directoriesScanned,
        total: directoriesScanned + next.length,
        subject: '${matches.length} exact RFI PDF',
      ),
    );
    directories = next;
  }
  for (final paths in matches.values) {
    paths.sort(_comparePaths);
    _dedupSorted(paths);
  }
  invalidNames.sort(_comparePaths);
  _dedupSorted(invalidNames);
  return _CompletionPdfDiscovery(
    matches: matches,
    invalidNames: invalidNames,
    workerCount: workerCount,
    directoriesScanned: directoriesScanned,
    entriesScanned: entriesScanned,
  );
}

/// Isolate entry for `scan_completion_directory`.
_CompletionDirectoryResult _scanCompletionDirectoryTask(
  (String, String) input,
) {
  try {
    return _CompletionDirectoryResult(
      _scanCompletionDirectory(input.$1, input.$2),
      null,
    );
  } on BackendError catch (error) {
    return _CompletionDirectoryResult(null, error);
  }
}

/// `scan_completion_directory`.
_CompletionDirectoryBatch _scanCompletionDirectory(
  String root,
  String directory,
) {
  final batch = _CompletionDirectoryBatch();
  final List<FileSystemEntity> entries;
  try {
    entries = Directory(directory).listSync(followLinks: false);
  } on Object {
    throw BackendError.pathInvalid('RFI system folder could not be read');
  }
  for (final entry in entries) {
    batch.entriesScanned += 1;
    if (entry is Link) {
      continue;
    }
    if (entry is Directory) {
      batch.directories.add(entry.path);
      continue;
    }
    if (entry is! File) {
      continue;
    }
    final name = p.basename(entry.path);
    final lower = _asciiLower(name);
    final isNamedPdf = lower.startsWith('rfi no. ') && lower.endsWith('.pdf');
    final code = _completionCodeFromFilename(name);
    if (code == null) {
      if (isNamedPdf) {
        batch.invalidNames.add(entry.path);
      }
      continue;
    }
    final String path;
    try {
      path = _canonicalize(entry.path);
    } on Object {
      throw BackendError.pathInvalid('RFI PDF could not be resolved');
    }
    if (!_pathStartsWith(path, root) || !FileSystemEntity.isFileSync(path)) {
      throw BackendError.pathEscape();
    }
    batch.matches.add((code, path));
  }
  batch.directories.sort(_comparePaths);
  batch.matches.sort((left, right) {
    final byCode = _compareUtf8(left.$1, right.$1);
    return byCode != 0 ? byCode : _comparePaths(left.$2, right.$2);
  });
  batch.invalidNames.sort(_comparePaths);
  return batch;
}

// ---------------------------------------------------------------------------
// Workbook layout discovery
// ---------------------------------------------------------------------------

String _cleanHeader(String value) => asciiUpper(_rsTrim(unorm.nfc(value)));

bool _cellIsMerged(WorksheetData sheet, int row, int col) =>
    sheet.mergedRanges.any(
      (range) =>
          range.firstRow <= row &&
          row <= range.lastRow &&
          range.firstColumn <= col &&
          col <= range.lastColumn,
    );

_HeaderLayout _discoverHeader(
  WorksheetData sheet,
  List<String> acceptedValueHeaders,
) {
  final layouts = <_HeaderLayout>[];
  final maxRow = sheet.maxRow < _maxHeaderRows ? sheet.maxRow : _maxHeaderRows;
  final maxColumn = sheet.maxColumn < _maxHeaderColumns
      ? sheet.maxColumn
      : _maxHeaderColumns;
  for (var row = 1; row <= maxRow; row++) {
    final rfiColumns = <int>[];
    final valueColumns = <int>[];
    for (var col = 1; col <= maxColumn; col++) {
      final cell = sheet.cell(row, col);
      if (cell == null) {
        continue;
      }
      final header = _cleanHeader(cell.value);
      if (header == 'RFI NO.' || header == 'RFI NO') {
        rfiColumns.add(col);
      }
      if (acceptedValueHeaders.contains(header)) {
        valueColumns.add(col);
      }
    }
    if (rfiColumns.length == 1 && valueColumns.length == 1) {
      layouts.add(_HeaderLayout(row, rfiColumns[0], valueColumns[0]));
    }
  }
  if (layouts.length != 1) {
    throw BackendError.validation('error.rfi.systemSync.workbookLayoutInvalid');
  }
  return layouts[0];
}

// ---------------------------------------------------------------------------
// System scan
// ---------------------------------------------------------------------------

/// `scan_system_root_indexed`: discovers `<DISC>/<RFI folder>/RFI No. ….pdf`
/// candidates and detects Worley signatures, reusing durable index outcomes.
Future<(List<ScannedRfiRecord>, ScanIndexStats)> scanSystemRootIndexed(
  String root,
  ScanIndex? scanIndex,
  String ownerId,
  bool forceRescan,
  void Function(ProgressEvent) progress, {
  PdfPageRenderer? renderer,
}) async {
  if (!FileSystemEntity.isDirectorySync(root)) {
    throw BackendError.pathInvalid('RFI system root is not a directory: $root');
  }
  final searchDirs = <(String, String)>[];
  final List<FileSystemEntity> rootEntries;
  try {
    rootEntries = Directory(root).listSync(followLinks: false);
  } on FileSystemException catch (error) {
    throw BackendError.pathInvalid(_ioMessage(error));
  }
  for (final entry in rootEntries) {
    if (!FileSystemEntity.isDirectorySync(entry.path)) {
      continue;
    }
    final name = asciiUpper(p.basename(entry.path));
    final discipline = disciplines.where((value) => value == name).firstOrNull;
    if (discipline != null) {
      searchDirs.add((discipline, entry.path));
    }
  }
  if (searchDirs.isEmpty) {
    final name = asciiUpper(p.basename(root));
    final discipline = disciplines.where((value) => value == name).firstOrNull;
    if (discipline != null) {
      searchDirs.add((discipline, root));
    }
  }

  progress(
    ProgressEvent(phase: 'system_discovery', done: 0, total: 0, subject: root),
  );
  final candidates = <(ScannedRfiRecord, String)>[];
  final seen = <String>{};
  var discoveredDirs = 0;
  final int discoveryWorkers;
  final List<_DisciplineResult> batches;
  try {
    (
      discoveryWorkers,
      batches,
    ) = await boundedMapStream<(String, String), _DisciplineResult>(
      searchDirs,
      _scanSystemDisciplineTask,
      (index, _) {
        discoveredDirs += 1;
        progress(
          ProgressEvent(
            phase: 'system_discovery',
            done: discoveredDirs,
            total: searchDirs.length,
            subject: searchDirs[index].$2,
          ),
        );
      },
    );
  } on Object catch (error) {
    throw BackendError.requestFailed('$error');
  }
  for (final batch in batches) {
    final error = batch.error;
    if (error != null) {
      throw BackendError.pathInvalid(error);
    }
    for (final (record, folder) in batch.candidates!) {
      if (!seen.add(record.canonicalRfi)) {
        continue;
      }
      candidates.add((record, folder));
    }
  }
  final tempDir = Directory.systemTemp.createTempSync('docflow-system-sync-');
  try {
    _stableSort(candidates, (left, right) {
      final byDiscipline = _compareUtf8(
        left.$1.discipline,
        right.$1.discipline,
      );
      return byDiscipline != 0
          ? byDiscipline
          : left.$1.number.compareTo(right.$1.number);
    });
    progress(
      ProgressEvent(
        phase: 'system_discovery',
        done: candidates.length,
        total: candidates.length,
        subject: '',
      ),
    );
    var reusedCount = 0;
    var analyzedCount = 0;
    var failedCount = 0;
    var priorInterruptedRuns = 0;
    ScanRun? run;
    if (scanIndex != null) {
      try {
        final (begun, prior) = scanIndex.beginRun(
          ownerId,
          operationId,
          _systemScanProfile,
          root,
          forceRescan: forceRescan,
        );
        run = begun;
        priorInterruptedRuns = prior;
      } on Object catch (error) {
        throw BackendError.requestFailed(
          'scan index unavailable: ${_errorText(error)}',
        );
      }
    }
    try {
      final total = candidates.length;
      var completed = 0;
      final activeRun = run;

      Future<_CandidateAnalysis> analyze(
        (ScannedRfiRecord, String) item,
      ) async {
        final (source, rfiFolder) = item;
        final record = source.copy();
        final pdfPath = record.pdfPath;
        if (pdfPath == null) {
          record.signatureReason = 'Exact RFI No. PDF not found in folder';
          return _CandidateAnalysis(
            record: record,
            relative: null,
            fingerprint: null,
            detection: null,
            reused: false,
            transientFailure: false,
            subject: rfiFolder,
          );
        }
        if (!_pathStartsWith(pdfPath, root)) {
          throw const _CandidateAnalysisError(
            null,
            'RFI scan escaped the selected system root',
          );
        }
        final relative = p.relative(pdfPath, from: root);
        final FileFingerprint fingerprint;
        try {
          fingerprint = FileFingerprint.read(pdfPath);
        } on Object catch (error) {
          throw _CandidateAnalysisError(
            relative,
            'RFI scan fingerprint failed: ${_errorText(error)}',
          );
        }
        _CachedWorleyDetection? cached;
        if (!forceRescan && activeRun != null) {
          try {
            cached = activeRun.lookup<_CachedWorleyDetection>(
              relative,
              fingerprint,
              _CachedWorleyDetection.fromJson,
            );
          } on Object catch (error) {
            throw _CandidateAnalysisError(
              relative,
              'scan index read failed: ${_errorText(error)}',
            );
          }
        }
        if (cached != null && cached.signedByWorley) {
          final String currentHash;
          try {
            currentHash = fileSha256(pdfPath);
          } on Object catch (error) {
            throw _CandidateAnalysisError(
              relative,
              'signed RFI hash failed: ${_errorText(error)}',
            );
          }
          if (cached.sourceSha256 != currentHash) {
            cached = null;
          }
        }
        final _CachedWorleyDetection detection;
        final bool reused;
        final bool transientFailure;
        if (cached != null) {
          detection = cached;
          reused = true;
          transientFailure = false;
        } else {
          final (signedByWorley, signatureReason) = await detectWorleySignature(
            pdfPath,
            tempDir.path,
            renderer: renderer,
          );
          final FileFingerprint currentFingerprint;
          try {
            currentFingerprint = FileFingerprint.read(pdfPath);
          } on Object catch (error) {
            throw _CandidateAnalysisError(
              relative,
              'RFI scan fingerprint changed: ${_errorText(error)}',
            );
          }
          if (currentFingerprint != fingerprint) {
            throw _CandidateAnalysisError(
              relative,
              'RFI PDF changed during system scan; preview again',
            );
          }
          String? sourceSha256;
          if (signedByWorley) {
            try {
              sourceSha256 = fileSha256(pdfPath);
            } on Object catch (error) {
              throw _CandidateAnalysisError(
                relative,
                'signed RFI hash failed: ${_errorText(error)}',
              );
            }
          }
          detection = _CachedWorleyDetection(
            signedByWorley: signedByWorley,
            signatureReason: signatureReason,
            sourceSha256: sourceSha256,
          );
          reused = false;
          transientFailure =
              detection.signatureReason.startsWith('PDF render error:') ||
              detection.signatureReason.startsWith('Image read error:');
        }
        record.signedByWorley = detection.signedByWorley;
        record.signatureReason = detection.signatureReason;
        return _CandidateAnalysis(
          record: record,
          relative: relative,
          fingerprint: fingerprint,
          detection: detection,
          reused: reused,
          transientFailure: transientFailure,
          subject: relative,
        );
      }

      void onCompleted(
        _CandidateAnalysis? item,
        _CandidateAnalysisError? error,
      ) {
        if (error != null) {
          final relative = error.relative;
          if (activeRun != null && relative != null) {
            try {
              activeRun.recordFailure(relative, error.message);
            } on Object catch (failure) {
              throw _PoolCallbackError(
                'scan checkpoint failed: ${_errorText(failure)}',
              );
            }
          }
          throw _PoolCallbackError(error.message);
        }
        final value = item!;
        if (value.reused) {
          reusedCount += 1;
        } else if (value.relative != null &&
            value.fingerprint != null &&
            value.detection != null) {
          if (activeRun != null) {
            try {
              if (value.transientFailure) {
                activeRun.recordFailure(
                  value.relative!,
                  value.detection!.signatureReason,
                );
              } else {
                activeRun.recordSuccess(
                  value.relative!,
                  value.fingerprint!,
                  value.detection!.toJson(),
                );
              }
            } on Object catch (failure) {
              throw _PoolCallbackError(
                'scan checkpoint failed: ${_errorText(failure)}',
              );
            }
          }
          if (value.transientFailure) {
            failedCount += 1;
          } else {
            analyzedCount += 1;
          }
        }
        completed += 1;
        progress(
          ProgressEvent(
            phase: 'system_signature_analysis',
            done: completed,
            total: total,
            subject: value.subject,
          ),
        );
      }

      final (workerCountValue, analyzed) = await _boundedAsyncMapStream(
        candidates,
        analyze,
        onCompleted,
      );
      final analysisWorkers = workerCountValue > discoveryWorkers
          ? workerCountValue
          : discoveryWorkers;
      final stats = ScanIndexStats(
        discovered: candidates.length,
        reused: reusedCount,
        analyzed: analyzedCount,
        failed: failedCount,
        priorInterruptedRuns: priorInterruptedRuns,
        forceRescan: forceRescan,
        parallelAnalysis: candidates.isNotEmpty,
        analysisWorkers: analysisWorkers,
      );
      final records = [for (final item in analyzed) item.record];
      if (activeRun != null) {
        try {
          activeRun.complete(stats);
        } on Object catch (error) {
          throw BackendError.requestFailed(
            'scan run completion failed: ${_errorText(error)}',
          );
        }
      }
      _stableSort(records, (left, right) {
        final byDiscipline = _compareUtf8(left.discipline, right.discipline);
        return byDiscipline != 0
            ? byDiscipline
            : left.number.compareTo(right.number);
      });
      return (records, stats);
    } finally {
      // Rust drops an unfinished run, which marks it interrupted.
      run?.abandon();
    }
  } finally {
    try {
      tempDir.deleteSync(recursive: true);
    } on Object {
      // TempDir drop ignores cleanup failures.
    }
  }
}

class _PoolCallbackError implements Exception {
  const _PoolCallbackError(this.message);

  final String message;
}

/// `bounded_map_stream` over an asynchronous [map] on the calling isolate:
/// at most [workerCount] items run concurrently, [completed] runs once per
/// item in completion order, and the first callback failure stops further
/// callbacks and new work, then fails the whole map with request_failed.
Future<(int, List<_CandidateAnalysis>)> _boundedAsyncMapStream(
  List<(ScannedRfiRecord, String)> items,
  Future<_CandidateAnalysis> Function((ScannedRfiRecord, String)) map,
  void Function(_CandidateAnalysis?, _CandidateAnalysisError?) completed,
) async {
  final workers = workerCount(items.length);
  final results = List<_CandidateAnalysis?>.filled(items.length, null);
  var next = 0;
  String? failure;

  Future<void> worker() async {
    while (failure == null && next < items.length) {
      final index = next++;
      _CandidateAnalysis? value;
      _CandidateAnalysisError? error;
      try {
        value = await map(items[index]);
      } on _CandidateAnalysisError catch (caught) {
        error = caught;
      }
      results[index] = value;
      if (failure != null) {
        return;
      }
      try {
        completed(value, error);
      } on _PoolCallbackError catch (caught) {
        failure ??= caught.message;
        return;
      }
    }
  }

  await Future.wait([for (var index = 0; index < workers; index++) worker()]);
  if (failure != null) {
    throw BackendError.requestFailed(failure!);
  }
  return (workers, [for (final result in results) result!]);
}

/// Isolate entry for `scan_system_discipline`.
_DisciplineResult _scanSystemDisciplineTask((String, String) input) {
  try {
    return _DisciplineResult(_scanSystemDiscipline(input.$1, input.$2), null);
  } on FileSystemException catch (error) {
    return _DisciplineResult(null, _ioMessage(error));
  }
}

/// `scan_system_discipline`.
List<(ScannedRfiRecord, String)> _scanSystemDiscipline(
  String discipline,
  String disciplinePath,
) {
  final entries = Directory(disciplinePath).listSync(followLinks: false);
  final candidates = <(ScannedRfiRecord, String)>[];
  for (final entry in entries) {
    if (entry is! Directory) {
      continue;
    }
    final rfiFolder = entry.path;
    final folderName = p.basename(rfiFolder);
    final parsed = parseRfiDisciplineAndNumber(folderName);
    if (parsed == null) {
      continue;
    }
    final (parsedDiscipline, number) = parsed;
    if (parsedDiscipline != discipline) {
      continue;
    }
    final canonicalRfi =
        '${canonicalRfiPrefix()}$parsedDiscipline-${_pad4(number)}';
    final expectedPdf = 'RFI No. $canonicalRfi.pdf';
    String? selectedPdf;
    try {
      for (final child in Directory(rfiFolder).listSync(followLinks: false)) {
        if (child is File &&
            _eqIgnoreAsciiCase(p.basename(child.path), expectedPdf)) {
          selectedPdf = child.path;
          break;
        }
      }
    } on FileSystemException {
      // `fs::read_dir(..).into_iter().flatten()` treats an unreadable folder
      // as having no PDF.
    }
    candidates.add((
      ScannedRfiRecord(
        discipline: parsedDiscipline,
        number: number,
        canonicalRfi: canonicalRfi,
        shortRfi: '$parsedDiscipline-${_pad4(number)}',
        rfiFolder: rfiFolder,
        pdfPath: selectedPdf,
        signedByWorley: false,
        signatureReason: '',
      ),
      rfiFolder,
    ));
  }
  return candidates;
}

/// `detect_worley_signature`: inspects only the inner name/signature area of
/// the approved form's bottom-right OTHERS cell.
Future<(bool, String)> detectWorleySignature(
  String pdfPath,
  String tempDir, {
  PdfPageRenderer? renderer,
}) async {
  final pngPath = p.join(tempDir, '${_uuid.v4()}.png');
  try {
    await renderFirstPagePng(
      pdfPath,
      pngPath,
      150,
      renderer: renderer ?? selectPdfPageRenderer(),
    );
  } on Object catch (error) {
    return (false, 'PDF render error: ${_errorText(error)}');
  }
  final GrayImage image;
  try {
    image = readPngGray(pngPath);
  } on Object catch (error) {
    return (false, 'Image read error: ${_errorText(error)}');
  }
  try {
    File(pngPath).deleteSync();
  } on Object {
    // `let _ = fs::remove_file(&png_path);`
  }
  final imageWidth = image.width;
  final imageHeight = image.height;
  final pixels = image.pixels;
  if (imageWidth < 100 || imageHeight < 100) {
    return (false, 'Image dimensions too small');
  }
  // The approved form's Worley evidence is the inner name/signature area of
  // the bottom-right OTHERS cell. Everything outside this region—including
  // contractor signatures and the general-notes area—is non-authoritative.
  final x1 = (imageWidth * 0.74).toInt();
  final x2 = (imageWidth * 0.92).toInt();
  final y1 = (imageHeight * 0.72).toInt();
  final y2 = (imageHeight * 0.77).toInt();
  final width = x2 > x1 ? x2 - x1 : 0;
  final height = y2 > y1 ? y2 - y1 : 0;
  if (width == 0 || height == 0) {
    return (false, 'Empty OTHERS inspection area');
  }
  final yEnd = y2 < imageHeight ? y2 : imageHeight;
  final xEnd = x2 < imageWidth ? x2 : imageWidth;
  final rowDark = List<int>.filled(height, 0);
  final columnDark = List<int>.filled(width, 0);
  for (var y = y1; y < yEnd; y++) {
    final row = y - y1;
    for (var x = x1; x < xEnd; x++) {
      final column = x - x1;
      if (pixels[y * imageWidth + x] < 175) {
        rowDark[row] += 1;
        columnDark[column] += 1;
      }
    }
  }
  final ignoredRows = [for (final dark in rowDark) dark / width >= 0.35];
  final ignoredColumns = [for (final dark in columnDark) dark / height >= 0.35];
  var dark = 0;
  var total = 0;
  for (var y = y1; y < yEnd; y++) {
    final row = y - y1;
    for (var x = x1; x < xEnd; x++) {
      final column = x - x1;
      if (ignoredRows[row] || ignoredColumns[column]) {
        continue;
      }
      total += 1;
      if (pixels[y * imageWidth + x] < 175) {
        dark += 1;
      }
    }
  }
  if (total == 0) {
    return (false, 'Empty OTHERS inspection area');
  }
  final ratio = dark / total;
  final signed = dark >= 500 && ratio >= 0.025;
  final reason = signed
      ? 'Signed by Worley (OTHERS ink: $dark px, '
            '${(ratio * 100.0).toStringAsFixed(1)}%)'
      : 'Unsigned / OTHERS empty (ink: $dark px, '
            '${(ratio * 100.0).toStringAsFixed(2)}%)';
  return (signed, reason);
}

// ---------------------------------------------------------------------------
// Workbook readers
// ---------------------------------------------------------------------------

List<_RfiLogRowInfo> _readRfiLogRows(WorkbookData workbook) {
  final rows = <_RfiLogRowInfo>[];
  for (final discipline in disciplines) {
    final sheet = workbook.sheet(discipline);
    if (sheet == null) {
      throw BackendError.validation(
        'error.rfi.systemSync.workbookLayoutInvalid',
      );
    }
    final layout = _discoverHeader(sheet, const ['REMARKS']);
    for (var row = layout.row + 1; row <= sheet.maxRow; row++) {
      final rfiCell = sheet.cell(row, layout.rfiCol);
      if (rfiCell == null) {
        continue;
      }
      final canonicalRfi = exactCanonicalRfi(rfiCell.value);
      if (canonicalRfi == null) {
        continue;
      }
      if (!canonicalRfi.contains('-RFI-$discipline-')) {
        continue;
      }
      rows.add(
        _RfiLogRowInfo(
          sheet: discipline,
          row: row,
          canonicalRfi: canonicalRfi,
          remarksCol: layout.valueCol,
          remarksCell: sheet.cell(row, layout.valueCol),
          remarksMerged: _cellIsMerged(sheet, row, layout.valueCol),
        ),
      );
    }
  }
  return rows;
}

List<_RfiLogStatusRowInfo> _readRfiLogStatusRows(WorkbookData workbook) {
  final rows = <_RfiLogStatusRowInfo>[];
  for (final discipline in disciplines) {
    final sheet = workbook.sheet(discipline);
    if (sheet == null) {
      throw BackendError.validation(
        'error.rfi.completeStatus.workbookLayoutInvalid',
      );
    }
    final _HeaderLayout layout;
    try {
      layout = _discoverHeader(sheet, const ['INSPECTION STATUS']);
    } on BackendError {
      throw BackendError.validation(
        'error.rfi.completeStatus.workbookLayoutInvalid',
      );
    }
    for (var row = layout.row + 1; row <= sheet.maxRow; row++) {
      final rfiCell = sheet.cell(row, layout.rfiCol);
      if (rfiCell == null) {
        continue;
      }
      final canonicalRfi = exactCanonicalRfi(rfiCell.value);
      if (canonicalRfi == null) {
        continue;
      }
      if (!canonicalRfi.contains('-RFI-$discipline-')) {
        continue;
      }
      rows.add(
        _RfiLogStatusRowInfo(
          sheet: discipline,
          row: row,
          canonicalRfi: canonicalRfi,
          statusCol: layout.valueCol,
          statusCell: sheet.cell(row, layout.valueCol),
          statusMerged: _cellIsMerged(sheet, row, layout.valueCol),
        ),
      );
    }
  }
  return rows;
}

Map<String, List<_FollowUpTarget>> _readFollowUpTargets(WorkbookData workbook) {
  final targets = SplayTreeMap<String, List<_FollowUpTarget>>(_compareUtf8);
  for (final discipline in const ['CIV', 'MEC']) {
    final candidates = workbook.sheets.entries.where((entry) {
      final name = _cleanHeader(entry.key);
      return name.contains('SIGNED BY WORLEY') && name.endsWith(discipline);
    }).toList();
    if (candidates.length != 1) {
      throw BackendError.validation(
        'error.rfi.systemSync.followUpLayoutInvalid',
      );
    }
    final MapEntry(key: sheetName, value: sheet) = candidates[0];
    final valueHeaders = discipline == 'MEC'
        ? const ['WORLEY STATUS']
        : const ['STATUS'];
    final layout = _discoverHeader(sheet, valueHeaders);
    for (var row = layout.row + 1; row <= sheet.maxRow; row++) {
      final rfiCell = sheet.cell(row, layout.rfiCol);
      if (rfiCell == null) {
        continue;
      }
      final short = _cleanHeader(rfiCell.value);
      final parsed = parseRfiDisciplineAndNumber(short);
      if (parsed == null) {
        continue;
      }
      final (parsedDiscipline, number) = parsed;
      if (parsedDiscipline != discipline ||
          short != '$discipline-${_pad4(number)}') {
        continue;
      }
      final canonical = '${canonicalRfiPrefix()}$short';
      targets
          .putIfAbsent(canonical, () => <_FollowUpTarget>[])
          .add(
            _FollowUpTarget(
              sheet: sheetName,
              row: row,
              statusCol: layout.valueCol,
              statusCell: sheet.cell(row, layout.valueCol),
              statusMerged: _cellIsMerged(sheet, row, layout.valueCol),
            ),
          );
    }
  }
  return targets;
}

bool _safeCell(CellData? cell, bool merged) =>
    cell != null && cell.formula == null && !merged;

GuardedCellUpdateDto _dto(
  String rowId,
  CellData cell,
  String replacement,
  int row,
  int col,
) => GuardedCellUpdateDto(
  rowId: rowId,
  expectedValue: cell.value,
  expectedFormula: cell.formula,
  expectedStyle: cell.style,
  replacementValue: replacement,
  row: row,
  col: col,
);

String _appendSignedMarker(String value) {
  if (_rustLines(value).any((line) => _rsTrim(line) == worleySignedMarker)) {
    return value;
  }
  if (value.isEmpty) {
    return worleySignedMarker;
  }
  return '$value\n$worleySignedMarker';
}

// ---------------------------------------------------------------------------
// Workbook files
// ---------------------------------------------------------------------------

/// Opens [path] without following links and captures its bytes, identity and
/// mtime from one handle, exactly as `FilesystemAuthority` fingerprints the
/// workbook before applying a sync step.
FollowUpWorkbookFile openSyncWorkbookFile(String path) {
  final OsHandle handle;
  try {
    handle = openSourceNoFollow(path);
  } on Object {
    throw BackendError.validation('error.rfi.systemSync.workbookLocked');
  }
  try {
    final FilesystemObjectIdentity identity = fileIdentity(handle);
    final stat = handle.metadata();
    final builder = BytesBuilder(copy: false);
    final buffer = Uint8List(64 * 1024);
    handle.seek(0);
    while (true) {
      final count = handle.read(buffer);
      if (count == 0) break;
      builder.add(Uint8List.fromList(Uint8List.sublistView(buffer, 0, count)));
    }
    handle.seek(0);
    return FollowUpWorkbookFile(
      path: path,
      bytes: builder.takeBytes(),
      identity: switch (identity) {
        UnixObjectIdentity(:final device, :final inode) =>
          FollowUpFileIdentity.unix(device: device, inode: inode),
        WindowsObjectIdentity(:final volumeSerial, :final fileIndex) =>
          FollowUpFileIdentity.windows(
            volumeSerial: volumeSerial,
            fileIndex: fileIndex,
          ),
      },
      mtimeNs: stat.mtimeNs,
    );
  } finally {
    handle.close();
  }
}

// ---------------------------------------------------------------------------
// Completion status preview
// ---------------------------------------------------------------------------

/// `preview_complete_status_sync`: recursively finds exact `RFI No. <code>.pdf`
/// files and proposes `Completed` for the matching safe RFI Log Inspection
/// Status cells.
Future<Plan> previewCompleteStatusSync(
  String systemRoot,
  String rfiLogPath,
  FollowUpWorkbookFile rfiLogFile,
  void Function(ProgressEvent) progress,
  bool Function() cancelled,
) async {
  final discovery = await _discoverCompletionPdfs(
    systemRoot,
    progress,
    cancelled,
  );
  progress(
    ProgressEvent(
      phase: 'completion_workbook_read',
      done: 0,
      total: 1,
      subject: rfiLogPath,
    ),
  );
  final fingerprint = fingerprintWorkbookFile(rfiLogFile);
  final WorkbookData workbook;
  try {
    workbook = readWorkbook(rfiLogFile.bytes);
  } on Object catch (error) {
    throw BackendError.validation(
      'error.rfi.completeStatus.workbookInvalid: ${_errorText(error)}',
    );
  }
  final logRows = _readRfiLogStatusRows(workbook);
  progress(
    ProgressEvent(
      phase: 'completion_workbook_read',
      done: 1,
      total: 1,
      subject: rfiLogPath,
    ),
  );

  final updates = _updatesMap();
  final reviewRows = <JsonObject>[];
  final total = discovery.matches.length;
  var index = 0;
  for (final MapEntry(key: canonicalRfi, value: pdfPaths)
      in discovery.matches.entries) {
    progress(
      ProgressEvent(
        phase: 'completion_mapping',
        done: index,
        total: total,
        subject: canonicalRfi,
      ),
    );
    final rowId = 'complete-row-${index + 1}';
    index++;
    final matching = logRows
        .where((row) => row.canonicalRfi == canonicalRfi)
        .toList();
    final currentValues = SplayTreeSet<String>(_compareUtf8)
      ..addAll([
        for (final row in matching)
          if (row.statusCell != null) _rsTrim(unorm.nfc(row.statusCell!.value)),
      ]);
    final currentStatus = currentValues.isEmpty
        ? 'Missing cell'
        : currentValues.join(' / ');
    final safeCells =
        matching.isNotEmpty &&
        matching.every((row) => _safeCell(row.statusCell, row.statusMerged));
    final knownValues = currentValues.every(
      (value) => value == '' || value == 'Ongoing' || value == 'Completed',
    );
    final pending = <(String, String, GuardedCellUpdateDto)>[];
    if (safeCells && knownValues) {
      for (final row in matching) {
        final cell = row.statusCell!;
        if (_rsTrim(unorm.nfc(cell.value)) != 'Completed') {
          pending.add((
            row.sheet,
            '${row.row}:${row.statusCol}',
            _dto(rowId, cell, 'Completed', row.row, row.statusCol),
          ));
        }
      }
    }
    final String disposition;
    final String action;
    final String reason;
    final bool selectable;
    if (matching.isEmpty) {
      disposition = 'rfi.completeStatus.disposition.manualReview';
      action = 'RFI was not found in the RFI Log';
      reason = 'Exact completed PDF exists, but no exact RFI Log row was found';
      selectable = false;
    } else if (!safeCells) {
      disposition = 'rfi.completeStatus.disposition.manualReview';
      action = 'Inspection Status cell is missing, merged, or formula-bearing';
      reason = 'No unsafe workbook cell was authorized';
      selectable = false;
    } else if (!knownValues) {
      disposition = 'rfi.completeStatus.disposition.manualReview';
      action = 'Current status conflicts with automatic completion';
      reason = 'Only blank, Ongoing, or Completed status cells may be synchronized automatically';
      selectable = false;
    } else if (pending.isEmpty) {
      disposition = 'rfi.completeStatus.disposition.alreadyCompleted';
      action = 'Already Completed';
      reason = 'Every matching RFI Log row is already Completed';
      selectable = false;
    } else {
      disposition = 'rfi.completeStatus.disposition.ready';
      action = 'Set matching Inspection Status cell(s) to Completed';
      reason = 'Exact completed RFI PDF and safe RFI Log mapping found';
      selectable = true;
    }
    if (selectable) {
      for (final (sheet, key, update) in pending) {
        updates.putIfAbsent(sheet, _cellUpdatesMap)[key] = update;
      }
    }
    final parsed = parseRfiDisciplineAndNumber(canonicalRfi);
    if (parsed == null) {
      throw StateError('exact canonical completion code is parseable');
    }
    final (discipline, number) = parsed;
    final relativePaths = [
      for (final path in pdfPaths)
        if (_pathStartsWith(path, systemRoot)) _stripPrefix(path, systemRoot),
    ];
    reviewRows.add({
      '_rowId': rowId,
      'discipline': discipline,
      'rfiNo': '$discipline-${_pad4(number)}',
      'canonicalRfi': canonicalRfi,
      'pdfPath': relativePaths.isNotEmpty ? relativePaths.first : pdfPaths[0],
      'pdfCount': pdfPaths.length,
      'pdfPaths': relativePaths.join('\n'),
      'currentStatus': currentStatus,
      'proposedStatus': 'Completed',
      'action': action,
      'disposition': disposition,
      'reasonKey': reason,
      'enabled': selectable,
      'selectable': selectable,
    });
  }
  progress(
    ProgressEvent(
      phase: 'completion_mapping',
      done: total,
      total: total,
      subject: '',
    ),
  );

  final steps = <Step>[];
  final count = _updateCount(updates);
  if (count > 0) {
    final backupPath = _backupPath(
      rfiLogPath,
      '.docflow-completion-sync-backup-',
    );
    final updateRowIds = _updateRowIds(updates);
    final canonicalRfiCodes = [
      for (final row in reviewRows)
        if (row['_rowId'] is String && updateRowIds.contains(row['_rowId']))
          if (row['canonicalRfi'] is String) row['canonicalRfi']! as String,
    ];
    final stepMeta = SystemSyncStepMeta(
      targetPath: rfiLogPath,
      backupPath: backupPath,
      sourceFingerprint: fingerprint,
      workbookKind: SyncWorkbookKind.rfiLogStatus,
      updates: updates,
      canonicalRfiCodes: canonicalRfiCodes,
      summary: 'Update $count RFI Log Inspection Status cell(s)',
    );
    steps.add(
      Step(
        kind: StepKind.workbookUpdate,
        src: rfiLogPath,
        dst: backupPath,
        noteKey: 'note.rfi.completeStatus.rfiLog',
        meta: {'syncStep': stepMeta.toJson()},
      ),
    );
  }

  final warnings = [
    for (final path in discovery.invalidNames.take(_maxCompletionWarnings))
      Warning(key: 'warn.rfi.completeStatus.invalidPdfName', subject: path),
  ];
  if (discovery.invalidNames.length > _maxCompletionWarnings) {
    warnings.add(
      Warning(
        key: 'warn.rfi.completeStatus.invalidPdfNameOverflow',
        subject:
            '${discovery.invalidNames.length - _maxCompletionWarnings} additional invalid RFI PDF name(s)',
      ),
    );
  }
  if (steps.isEmpty) {
    warnings.add(
      const Warning(
        key: 'warn.rfi.completeStatus.noChangesNeeded',
        subject: 'No safe RFI Log status changes are ready',
      ),
    );
  }
  final columns = <JsonObject>[
    {
      'key': 'enabled',
      'labelKey': 'rfi.completeStatus.column.enabled',
      'kind': 'bool',
      'bulk': true,
      'disabledWhen': {'field': 'selectable', 'equals': false},
    },
    {
      'key': 'discipline',
      'labelKey': 'rfi.completeStatus.column.discipline',
      'kind': 'readonly',
    },
    {
      'key': 'rfiNo',
      'labelKey': 'rfi.completeStatus.column.rfiNo',
      'kind': 'readonly',
    },
    {
      'key': 'pdfPath',
      'labelKey': 'rfi.completeStatus.column.pdfPath',
      'kind': 'readonly',
    },
    {
      'key': 'pdfCount',
      'labelKey': 'rfi.completeStatus.column.pdfCount',
      'kind': 'readonly',
    },
    {
      'key': 'currentStatus',
      'labelKey': 'rfi.completeStatus.column.currentStatus',
      'kind': 'readonly',
    },
    {
      'key': 'proposedStatus',
      'labelKey': 'rfi.completeStatus.column.proposedStatus',
      'kind': 'readonly',
    },
    {
      'key': 'disposition',
      'labelKey': 'rfi.completeStatus.column.disposition',
      'kind': 'readonly',
      'display': 'localizedKey',
    },
    {
      'key': 'action',
      'labelKey': 'rfi.completeStatus.column.action',
      'kind': 'readonly',
      'detail': true,
    },
    {
      'key': 'pdfPaths',
      'labelKey': 'rfi.completeStatus.column.pdfPaths',
      'kind': 'readonly',
      'detail': true,
    },
    {
      'key': 'reasonKey',
      'labelKey': 'rfi.completeStatus.column.reason',
      'kind': 'readonly',
      'detail': true,
    },
  ];
  final editor = <String, Object?>{
    'kind': 'recordTable',
    'layout': 'compact',
    'titleKey': 'rfi.completeStatus.editor.title',
    'rowCountKey': 'rfi.completeStatus.editor.rowCount',
    'requiredFields': ['enabled'],
    'groupField': 'discipline',
    'selectionGroupFields': <Object?>[],
    'columns': columns,
    'rows': reviewRows,
    'duplicateEnabled': false,
    'selectionEnabled': true,
    'bulkEnabled': true,
    'reorderable': false,
    'detailsEnabled': true,
    'detailsDefaultVisible': false,
    'copyEnabled': false,
  };
  final exactPdfCount = discovery.matches.values.fold<int>(
    0,
    (sum, paths) => sum + paths.length,
  );
  final meta = <String, Object?>{
    'editor': editor,
    'workerCount': discovery.workerCount,
    'directoriesScanned': discovery.directoriesScanned,
    'entriesScanned': discovery.entriesScanned,
    'exactPdfCount': exactPdfCount,
  };
  progress(
    const ProgressEvent(phase: 'preview_ready', done: 1, total: 1, subject: ''),
  );
  return Plan(
    planId: '',
    opId: completeStatusOperationId,
    steps: steps,
    warnings: warnings,
    meta: meta,
  );
}

// ---------------------------------------------------------------------------
// System sync preview
// ---------------------------------------------------------------------------

/// `preview_system_sync`: [previewSystemSyncIndexed] without a scan index.
Future<Plan> previewSystemSync(
  String systemRoot,
  String rfiLogPath,
  String followUpPath,
  FollowUpWorkbookFile rfiLogFile,
  FollowUpWorkbookFile followUpFile, {
  PdfPageRenderer? renderer,
}) => previewSystemSyncIndexed(
  systemRoot,
  rfiLogPath,
  followUpPath,
  rfiLogFile,
  followUpFile,
  null,
  'local',
  false,
  (_) {},
  renderer: renderer,
);

/// `preview_system_sync_indexed`.
Future<Plan> previewSystemSyncIndexed(
  String systemRoot,
  String rfiLogPath,
  String followUpPath,
  FollowUpWorkbookFile rfiLogFile,
  FollowUpWorkbookFile followUpFile,
  ScanIndex? scanIndex,
  String ownerId,
  bool forceRescan,
  void Function(ProgressEvent) progress, {
  PdfPageRenderer? renderer,
}) async {
  final (scannedRfis, scanStats) = await scanSystemRootIndexed(
    systemRoot,
    scanIndex,
    ownerId,
    forceRescan,
    progress,
    renderer: renderer,
  );
  progress(
    ProgressEvent(
      phase: 'system_workbook_read',
      done: 0,
      total: 2,
      subject: rfiLogPath,
    ),
  );
  final rfiLogFingerprint = fingerprintWorkbookFile(rfiLogFile);
  final WorkbookData rfiLogWorkbook;
  try {
    rfiLogWorkbook = readWorkbook(rfiLogFile.bytes);
  } on Object catch (error) {
    throw BackendError.validation(
      'error.rfi.systemSync.workbookInvalid: ${_errorText(error)}',
    );
  }
  final rfiLogRows = _readRfiLogRows(rfiLogWorkbook);
  progress(
    ProgressEvent(
      phase: 'system_workbook_read',
      done: 1,
      total: 2,
      subject: followUpPath,
    ),
  );

  final followUpFingerprint = fingerprintWorkbookFile(followUpFile);
  final WorkbookData followUpWorkbook;
  try {
    followUpWorkbook = readWorkbook(followUpFile.bytes);
  } on Object catch (error) {
    throw BackendError.validation(
      'error.rfi.systemSync.workbookInvalid: ${_errorText(error)}',
    );
  }
  final followUpTargets = _readFollowUpTargets(followUpWorkbook);
  progress(
    const ProgressEvent(
      phase: 'system_workbook_read',
      done: 2,
      total: 2,
      subject: '',
    ),
  );

  final logUpdates = _updatesMap();
  final followUpUpdates = _updatesMap();
  final reviewRows = <JsonObject>[];

  for (var index = 0; index < scannedRfis.length; index++) {
    final scanned = scannedRfis[index];
    progress(
      ProgressEvent(
        phase: 'system_mapping',
        done: index,
        total: scannedRfis.length,
        subject: scanned.canonicalRfi,
      ),
    );
    final rowId = 'sync-row-${index + 1}';
    final matchingLog = rfiLogRows
        .where((row) => row.canonicalRfi == scanned.canonicalRfi)
        .toList();
    final matchingFollowUp =
        followUpTargets[scanned.canonicalRfi] ?? const <_FollowUpTarget>[];
    final civOrMec = scanned.discipline == 'CIV' || scanned.discipline == 'MEC';
    var logAction = 'Not found in RFI Log';
    var followUpAction = civOrMec
        ? 'Not found in Follow Up control table'
        : 'Not applicable for ELE/PRC';
    var ready = false;
    var unsafeTarget = false;
    final pendingLogUpdates = <(String, String, GuardedCellUpdateDto)>[];
    final pendingFollowUpUpdates = <(String, String, GuardedCellUpdateDto)>[];

    if (scanned.signedByWorley && matchingLog.isNotEmpty) {
      if (matchingLog.every(
        (row) => _safeCell(row.remarksCell, row.remarksMerged),
      )) {
        var changed = 0;
        for (final row in matchingLog) {
          final cell = row.remarksCell!;
          final replacement = _appendSignedMarker(cell.value);
          if (replacement != cell.value) {
            pendingLogUpdates.add((
              row.sheet,
              '${row.row}:${row.remarksCol}',
              _dto(rowId, cell, replacement, row.row, row.remarksCol),
            ));
            changed += 1;
          }
        }
        logAction = changed == 0
            ? 'Already signed in RFI Log'
            : 'Add exact Worley remark to $changed RFI Log row(s)';
        ready = ready || changed > 0;
      } else {
        logAction =
            'Manual review: REMARKS cell missing, merged, or formula-bearing';
        unsafeTarget = true;
      }
    }

    if (matchingFollowUp.isNotEmpty) {
      if (matchingFollowUp.every(
        (target) => _safeCell(target.statusCell, target.statusMerged),
      )) {
        var changed = 0;
        for (final target in matchingFollowUp) {
          final cell = target.statusCell!;
          final current = _rsTrim(unorm.nfc(cell.value));
          if (current != '' && current != 'Signed' && current != 'Not Signed') {
            unsafeTarget = true;
            continue;
          }
          final String desired;
          if (scanned.signedByWorley) {
            desired = 'Signed';
          } else if (current == 'Signed') {
            // A weak/unsigned scan can never revoke a prior signature.
            desired = 'Signed';
          } else {
            desired = 'Not Signed';
          }
          if (current != desired) {
            pendingFollowUpUpdates.add((
              target.sheet,
              '${target.row}:${target.statusCol}',
              _dto(rowId, cell, desired, target.row, target.statusCol),
            ));
            changed += 1;
          }
        }
        if (unsafeTarget) {
          followUpAction = 'Manual review: unexpected Follow Up status';
        } else if (changed == 0) {
          followUpAction =
              'Already ${scanned.signedByWorley ? 'Signed' : 'Not Signed or preserved Signed'} in Follow Up';
        } else {
          followUpAction =
              'Set $changed Follow Up control status cell(s) to ${scanned.signedByWorley ? 'Signed' : 'Not Signed'}';
        }
        ready = ready || changed > 0;
      } else {
        followUpAction = 'Manual review: Follow Up status cell missing, merged, or formula-bearing';
        unsafeTarget = true;
      }
    } else if (civOrMec) {
      followUpAction = 'Manual review: no existing Follow Up control row';
      unsafeTarget = true;
    }

    if (unsafeTarget || matchingLog.isEmpty) {
      ready = false;
    } else {
      for (final (sheet, key, update) in pendingLogUpdates) {
        logUpdates.putIfAbsent(sheet, _cellUpdatesMap)[key] = update;
      }
      for (final (sheet, key, update) in pendingFollowUpUpdates) {
        followUpUpdates.putIfAbsent(sheet, _cellUpdatesMap)[key] = update;
      }
    }

    final String disposition;
    final String reasonKey;
    final bool selectable;
    if (unsafeTarget || matchingLog.isEmpty) {
      disposition = 'rfi.systemSync.disposition.manualReview';
      reasonKey = 'Unsafe or incomplete workbook mapping; no unsafe cell was authorized';
      selectable = false;
    } else if (ready) {
      disposition = 'rfi.systemSync.disposition.ready';
      reasonKey = scanned.signedByWorley
          ? 'Worley signature detected; exact guarded updates are ready'
          : 'No Worley signature detected; exact Not Signed update is ready';
      selectable = true;
    } else {
      disposition = 'rfi.systemSync.disposition.alreadySynced';
      reasonKey = 'Already synchronized';
      selectable = false;
    }
    // `SystemSyncRow` (serde camelCase, `_rowId`).
    reviewRows.add({
      '_rowId': rowId,
      'discipline': scanned.discipline,
      'rfiNo': scanned.shortRfi,
      'canonicalRfi': scanned.canonicalRfi,
      'worleyStatus': scanned.signedByWorley ? worleySignedMarker : 'Unsigned',
      'rfiLogAction': logAction,
      'followUpAction': followUpAction,
      'disposition': disposition,
      'reasonKey': reasonKey,
      'enabled': selectable,
      'selectable': selectable,
    });
  }

  final steps = <Step>[];
  for (final (kind, path, fingerprint, updates, note, label) in [
    (
      SyncWorkbookKind.rfiLog,
      rfiLogPath,
      rfiLogFingerprint,
      logUpdates,
      'note.rfi.systemSync.rfiLog',
      'RFI Log REMARKS',
    ),
    (
      SyncWorkbookKind.followUp,
      followUpPath,
      followUpFingerprint,
      followUpUpdates,
      'note.rfi.systemSync.followUp',
      'Follow Up status',
    ),
  ]) {
    final count = _updateCount(updates);
    if (count == 0) {
      continue;
    }
    final backupPath = _backupPath(path, '.docflow-system-sync-backup-');
    final updateRowIds = _updateRowIds(updates);
    final canonicalRfiCodes =
        (SplayTreeSet<String>(_compareUtf8)..addAll([
              for (final row in reviewRows)
                if (updateRowIds.contains(row['_rowId']))
                  row['canonicalRfi']! as String,
            ]))
            .toList();
    final stepMeta = SystemSyncStepMeta(
      targetPath: path,
      backupPath: backupPath,
      sourceFingerprint: fingerprint,
      workbookKind: kind,
      updates: updates,
      canonicalRfiCodes: canonicalRfiCodes,
      summary: 'Update $count $label cell(s)',
    );
    steps.add(
      Step(
        kind: StepKind.workbookUpdate,
        src: path,
        dst: backupPath,
        noteKey: note,
        meta: {'syncStep': stepMeta.toJson()},
      ),
    );
  }

  final warnings = steps.isEmpty
      ? [
          const Warning(
            key: 'warn.rfi.systemSync.noChangesNeeded',
            subject: 'All detected RFIs are already synchronized, unsigned, or require manual review',
          ),
        ]
      : <Warning>[];
  final columns = <JsonObject>[
    {
      'key': 'enabled',
      'labelKey': 'rfi.systemSync.column.enabled',
      'kind': 'bool',
      'bulk': false,
      'disabledWhen': {'field': 'selectable', 'equals': false},
    },
    {
      'key': 'discipline',
      'labelKey': 'rfi.systemSync.column.discipline',
      'kind': 'readonly',
    },
    {
      'key': 'rfiNo',
      'labelKey': 'rfi.systemSync.column.rfiNo',
      'kind': 'readonly',
    },
    {
      'key': 'worleyStatus',
      'labelKey': 'rfi.systemSync.column.worleyStatus',
      'kind': 'readonly',
    },
    {
      'key': 'rfiLogAction',
      'labelKey': 'rfi.systemSync.column.rfiLogAction',
      'kind': 'readonly',
    },
    {
      'key': 'followUpAction',
      'labelKey': 'rfi.systemSync.column.followUpAction',
      'kind': 'readonly',
    },
    {
      'key': 'disposition',
      'labelKey': 'rfi.systemSync.column.disposition',
      'kind': 'readonly',
      'display': 'localizedKey',
    },
    {
      'key': 'reasonKey',
      'labelKey': 'rfi.systemSync.column.reason',
      'kind': 'readonly',
      'detail': true,
    },
  ];
  final editor = <String, Object?>{
    'kind': 'recordTable',
    'layout': 'compact',
    'titleKey': 'rfi.systemSync.editor.title',
    'rowCountKey': 'rfi.systemSync.editor.rowCount',
    'requiredFields': ['enabled'],
    'groupField': 'discipline',
    'selectionGroupFields': <Object?>[],
    'columns': columns,
    'rows': reviewRows,
    'duplicateEnabled': false,
    'selectionEnabled': false,
    'bulkEnabled': false,
    'reorderable': false,
    'detailsEnabled': true,
    'detailsDefaultVisible': false,
    'copyEnabled': false,
  };
  final meta = <String, Object?>{
    'editor': editor,
    'scanIndex': scanStats.toJson(),
    'parallelAnalysis': scanStats.parallelAnalysis,
    'workerCount': scanStats.analysisWorkers,
  };
  progress(
    const ProgressEvent(phase: 'preview_ready', done: 1, total: 1, subject: ''),
  );
  return Plan(
    planId: '',
    opId: operationId,
    steps: steps,
    warnings: warnings,
    meta: meta,
  );
}

// ---------------------------------------------------------------------------
// Step metadata, revise, execute
// ---------------------------------------------------------------------------

/// `decode_step_meta`.
SystemSyncStepMeta decodeStepMeta(Step step) => _decodeStepMeta(step);

SystemSyncStepMeta _decodeStepMeta(Step step) {
  if (!step.meta.containsKey('syncStep')) {
    throw BackendError.validation('syncStep metadata missing in plan step');
  }
  try {
    return SystemSyncStepMeta.fromJson(step.meta['syncStep']);
  } on Object catch (error) {
    throw BackendError.validation(
      'Invalid syncStep metadata: ${_errorText(error)}',
    );
  }
}

/// `revise`: applies the user's row selection to a cached preview plan,
/// refusing any submitted row that differs from the preview except `enabled`.
Plan revise(Plan plan, List<Object?> rows) {
  final completion = plan.opId == completeStatusOperationId;
  final invalidKey = completion
      ? 'error.rfi.completeStatus.approvedUpdateInvalid'
      : 'error.rfi.systemSync.approvedUpdateInvalid';
  final noRowsKey = completion
      ? 'warn.rfi.completeStatus.noRowsSelected'
      : 'warn.rfi.systemSync.noRowsSelected';
  final editor = plan.meta['editor'];
  final originalRows = editor is Map ? editor['rows'] : null;
  if (originalRows is! List) {
    throw BackendError.validation(invalidKey);
  }
  if (rows.length != originalRows.length) {
    throw BackendError.validation(invalidKey);
  }
  final originals = SplayTreeMap<String, Object?>(_compareUtf8);
  for (final row in originalRows) {
    final rowId = row is Map ? row['_rowId'] : null;
    if (rowId is! String) {
      throw BackendError.validation(invalidKey);
    }
    originals[rowId] = row;
  }
  final enabled = <String>{};
  final revisedRows = <Object?>[];
  final seen = <String>{};
  for (final submitted in rows) {
    final rowId = submitted is Map ? submitted['_rowId'] : null;
    if (rowId is! String) {
      throw BackendError.validation(invalidKey);
    }
    if (!seen.add(rowId)) {
      throw BackendError.validation(invalidKey);
    }
    if (!originals.containsKey(rowId)) {
      throw BackendError.validation(invalidKey);
    }
    final original = originals[rowId];
    final requested = (submitted as Map)['enabled'];
    if (requested is! bool) {
      throw BackendError.validation(invalidKey);
    }
    final selectableValue = original is Map ? original['selectable'] : null;
    final selectable = selectableValue is bool ? selectableValue : false;
    if (requested && !selectable) {
      throw BackendError.validation(invalidKey);
    }
    final expected = _indexSet(_deepCopy(original), 'enabled', requested);
    if (!_jsonEquals(submitted, expected)) {
      throw BackendError.validation(invalidKey);
    }
    if (requested) {
      enabled.add(rowId);
    }
    revisedRows.add(expected);
  }

  final revisedMeta = _deepCopy(plan.meta)! as Map<String, Object?>;
  final revisedEditor = _indexSet(revisedMeta['editor'], 'rows', revisedRows);
  revisedMeta['editor'] = revisedEditor;
  final revisedSteps = <Step>[];
  for (final originalStep in plan.steps) {
    final meta = decodeStepMeta(originalStep);
    for (final updates in meta.updates.values) {
      updates.removeWhere((_, update) => !enabled.contains(update.rowId));
    }
    meta.updates.removeWhere((_, updates) => updates.isEmpty);
    if (meta.updates.isEmpty) {
      continue;
    }
    final updateRowIds = _updateRowIds(meta.updates);
    meta.canonicalRfiCodes =
        (SplayTreeSet<String>(_compareUtf8)..addAll([
              for (final row in originals.values)
                if (row is Map &&
                    row['_rowId'] is String &&
                    updateRowIds.contains(row['_rowId']) &&
                    row['canonicalRfi'] is String)
                  row['canonicalRfi']! as String,
            ]))
            .toList();
    final stepMeta = _deepCopy(originalStep.meta)! as Map<String, Object?>;
    stepMeta['syncStep'] = meta.toJson();
    revisedSteps.add(
      Step(
        kind: originalStep.kind,
        src: originalStep.src,
        dst: originalStep.dst,
        noteKey: originalStep.noteKey,
        meta: stepMeta,
      ),
    );
  }
  final revisedWarnings = [
    for (final warning in plan.warnings)
      if (warning.key != 'warn.rfi.systemSync.noRowsSelected' &&
          warning.key != 'warn.rfi.completeStatus.noRowsSelected')
        warning,
  ];
  if (revisedSteps.isEmpty) {
    revisedWarnings.add(
      Warning(
        key: noRowsKey,
        subject: completion
            ? 'No completed RFI rows are selected for synchronization'
            : 'No signed RFI rows are selected for synchronization',
      ),
    );
  }
  return Plan(
    planId: plan.planId,
    opId: plan.opId,
    steps: revisedSteps,
    warnings: revisedWarnings,
    meta: revisedMeta,
  );
}

/// `validate_replacement`: the closed replacement vocabulary per workbook.
bool _validateReplacement(SyncWorkbookKind kind, GuardedCellUpdateDto update) {
  switch (kind) {
    case SyncWorkbookKind.followUp:
      final expected = _rsTrim(update.expectedValue);
      return switch (update.replacementValue) {
        'Signed' =>
          expected == '' || expected == 'Signed' || expected == 'Not Signed',
        'Not Signed' => expected == '' || expected == 'Not Signed',
        _ => false,
      };
    case SyncWorkbookKind.rfiLogStatus:
      final expected = _rsTrim(unorm.nfc(update.expectedValue));
      return update.replacementValue == 'Completed' &&
          (expected == '' || expected == 'Ongoing' || expected == 'Completed');
    case SyncWorkbookKind.rfiLog:
      return update.replacementValue == worleySignedMarker ||
          update.replacementValue.endsWith('\n$worleySignedMarker') &&
              update.expectedValue.isNotEmpty &&
              update.replacementValue ==
                  '${update.expectedValue}\n$worleySignedMarker';
  }
}

Map<String, Map<(int, int), GuardedCellUpdate>> _guardedUpdates(
  SystemSyncStepMeta meta,
) {
  final updates = <String, Map<(int, int), GuardedCellUpdate>>{};
  for (final MapEntry(key: sheet, value: sheetUpdates)
      in meta.updates.entries) {
    for (final update in sheetUpdates.values) {
      if (update.row == 0 ||
          update.col == 0 ||
          update.expectedFormula != null ||
          !_validateReplacement(meta.workbookKind, update)) {
        throw BackendError.validation(
          meta.workbookKind == SyncWorkbookKind.rfiLogStatus
              ? 'error.rfi.completeStatus.approvedUpdateInvalid'
              : 'error.rfi.systemSync.approvedUpdateInvalid',
        );
      }
      final guarded = GuardedCellUpdate(
        expected: CellData(
          value: update.expectedValue,
          formula: update.expectedFormula,
          style: update.expectedStyle,
        ),
        replacement: CellUpdateText(update.replacementValue),
        createIfAbsent: false,
      );
      final sheetMap = updates.putIfAbsent(
        sheet,
        () => <(int, int), GuardedCellUpdate>{},
      );
      final key = (update.row, update.col);
      if (sheetMap.containsKey(key)) {
        throw BackendError.validation(
          'error.rfi.systemSync.approvedUpdateInvalid',
        );
      }
      sheetMap[key] = guarded;
    }
  }
  return updates;
}

/// `execute_sync_step` over bytes: validates the approved updates, patches the
/// [source] workbook and verifies the closed guarded delta. Returns the patched
/// workbook bytes.
Uint8List executeSyncStepBytes(Step step, Uint8List source) {
  final meta = decodeStepMeta(step);
  final updates = _guardedUpdates(meta);
  final Uint8List output;
  try {
    output = patchWorkbookSheetsGuardedText(source, updates);
  } on Object catch (error) {
    throw BackendError.validation(
      'Failed to patch workbook: ${_errorText(error)}',
    );
  }
  try {
    verifyGuardedWorkbookDeltaText(source, output, updates);
  } on Object catch (error) {
    throw BackendError.validation(
      'Workbook delta verification failed: ${_errorText(error)}',
    );
  }
  return output;
}

/// `execute_sync_step`: reads [sourcePath], writes the patched workbook to the
/// new file [destPath] (never overwriting) and verifies the delta.
void executeSyncStep(Step step, String sourcePath, String destPath) {
  final meta = decodeStepMeta(step);
  final updates = _guardedUpdates(meta);
  final Uint8List source;
  try {
    source = File(sourcePath).readAsBytesSync();
  } on FileSystemException catch (error) {
    throw BackendError.validation(
      'Failed to open staged workbook: ${_ioMessage(error)}',
    );
  }
  final RandomAccessFile destination;
  try {
    destination = createNewOutputFile(destPath);
  } on Object catch (error) {
    throw BackendError.validation(
      'Failed to create staged workbook: ${_errorText(error)}',
    );
  }
  try {
    final Uint8List output;
    try {
      output = patchWorkbookSheetsGuardedText(source, updates);
      writeOutputHandle(destination, output);
    } on Object catch (error) {
      throw BackendError.validation(
        'Failed to patch workbook: ${_errorText(error)}',
      );
    }
    final Uint8List written;
    try {
      written = readOutputHandle(destination);
    } on Object catch (error) {
      throw BackendError.protocol(_errorText(error));
    }
    try {
      verifyGuardedWorkbookDeltaText(source, written, updates);
    } on Object catch (error) {
      throw BackendError.validation(
        'Workbook delta verification failed: ${_errorText(error)}',
      );
    }
  } finally {
    destination.closeSync();
  }
}

/// The `rfi_system_sync` port consumed by `FilesystemAuthority`.
class RfiSystemSync implements RfiSystemSyncPort {
  const RfiSystemSync();

  @override
  SystemSyncStepMeta decodeStepMeta(Step step) => _decodeStepMeta(step);

  @override
  Uint8List executeSyncStep(Step step, Uint8List input) =>
      executeSyncStepBytes(step, input);
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

SplayTreeMap<String, SplayTreeMap<String, GuardedCellUpdateDto>>
_updatesMap() =>
    SplayTreeMap<String, SplayTreeMap<String, GuardedCellUpdateDto>>(
      _compareUtf8,
    );

SplayTreeMap<String, GuardedCellUpdateDto> _cellUpdatesMap() =>
    SplayTreeMap<String, GuardedCellUpdateDto>(_compareUtf8);

int _updateCount(Map<String, Map<String, GuardedCellUpdateDto>> updates) =>
    updates.values.fold<int>(0, (sum, cells) => sum + cells.length);

Set<String> _updateRowIds(
  Map<String, Map<String, GuardedCellUpdateDto>> updates,
) => {
  for (final cells in updates.values)
    for (final update in cells.values) update.rowId,
};

/// `path.parent().unwrap_or(".").join(<prefix><uuid simple>.<ext or xlsm>)`.
String _backupPath(String path, String prefix) {
  final extensionWithDot = p.extension(path);
  final extension = extensionWithDot.isEmpty
      ? 'xlsm'
      : extensionWithDot.substring(1);
  final name = '$prefix${_uuid.v4().replaceAll('-', '')}.$extension';
  final parent = p.dirname(path);
  if (parent == '.' && !path.startsWith('.${p.separator}') && path != '.') {
    // Rust `Path::parent()` of a bare file name is the empty path.
    return name;
  }
  return p.join(parent, name);
}

final RegExp _whiteSpaceRun = RegExp(
  r'^\p{White_Space}+|\p{White_Space}+$',
  unicode: true,
);

/// Rust `str::trim` (Unicode `White_Space`).
String _rsTrim(String value) => value.replaceAll(_whiteSpaceRun, '');

/// Rust `str::lines`: splits on `\n`, strips one trailing `\r` per line and
/// drops the final empty line.
List<String> _rustLines(String value) {
  if (value.isEmpty) {
    return const [];
  }
  final parts = value.split('\n');
  if (parts.last.isEmpty) {
    parts.removeLast();
  }
  return [
    for (final part in parts)
      part.endsWith('\r') ? part.substring(0, part.length - 1) : part,
  ];
}

String _asciiLower(String value) {
  final out = StringBuffer();
  for (final unit in value.codeUnits) {
    out.writeCharCode(unit >= 0x41 && unit <= 0x5A ? unit + 0x20 : unit);
  }
  return out.toString();
}

bool _eqIgnoreAsciiCase(String a, String b) => _asciiLower(a) == _asciiLower(b);

/// Rust `str` ordering (UTF-8 bytes).
int _compareUtf8(String a, String b) {
  final left = utf8.encode(a);
  final right = utf8.encode(b);
  final length = left.length < right.length ? left.length : right.length;
  for (var index = 0; index < length; index++) {
    final diff = left[index] - right[index];
    if (diff != 0) {
      return diff;
    }
  }
  return left.length - right.length;
}

/// Rust `PathBuf` ordering: component-wise.
int _comparePaths(String a, String b) {
  final left = p.split(a);
  final right = p.split(b);
  final length = left.length < right.length ? left.length : right.length;
  for (var index = 0; index < length; index++) {
    final diff = _compareUtf8(left[index], right[index]);
    if (diff != 0) {
      return diff;
    }
  }
  return left.length - right.length;
}

void _dedupSorted(List<String> values) {
  var write = 0;
  for (var read = 0; read < values.length; read++) {
    if (write == 0 || values[read] != values[write - 1]) {
      values[write++] = values[read];
    }
  }
  values.length = write;
}

/// Stable sort (Rust `sort_by` is stable; Dart `List.sort` is not).
void _stableSort<T>(List<T> values, int Function(T, T) compare) {
  final indexed = [for (var i = 0; i < values.length; i++) (i, values[i])];
  indexed.sort((left, right) {
    final order = compare(left.$2, right.$2);
    return order != 0 ? order : left.$1.compareTo(right.$1);
  });
  for (var i = 0; i < values.length; i++) {
    values[i] = indexed[i].$2;
  }
}

/// Rust `Path::starts_with`: component-wise prefix.
bool _pathStartsWith(String path, String root) {
  final pathParts = p.split(p.normalize(path));
  final rootParts = p.split(p.normalize(root));
  if (rootParts.length > pathParts.length) {
    return false;
  }
  for (var index = 0; index < rootParts.length; index++) {
    if (pathParts[index] != rootParts[index]) {
      return false;
    }
  }
  return true;
}

/// Rust `Path::strip_prefix(..).to_string_lossy()` for a path known to start
/// with [root].
String _stripPrefix(String path, String root) {
  final pathParts = p.split(p.normalize(path));
  final rootParts = p.split(p.normalize(root));
  final rest = pathParts.sublist(rootParts.length);
  return rest.isEmpty ? '' : p.joinAll(rest);
}

String _canonicalize(String path) {
  final type = FileSystemEntity.typeSync(path, followLinks: true);
  if (type == FileSystemEntityType.directory) {
    return Directory(path).resolveSymbolicLinksSync();
  }
  if (type == FileSystemEntityType.file) {
    return File(path).resolveSymbolicLinksSync();
  }
  throw FileSystemException('path cannot be canonicalized', path);
}

String _ioMessage(FileSystemException error) {
  final os = error.osError;
  if (os != null) {
    return '${os.message} (os error ${os.errorCode})';
  }
  return error.message;
}

String _errorText(Object error) {
  if (error is BackendError) {
    return error.message;
  }
  if (error is OcrFailure) {
    return error.message;
  }
  if (error is OoxmlError) {
    return error.message;
  }
  if (error is ScanIndexException) {
    return error.message;
  }
  if (error is FormatException) {
    return error.message;
  }
  if (error is FileSystemException) {
    return _ioMessage(error);
  }
  return '$error';
}

Map<String, Object?> _strictObject(
  Object? value,
  Set<String> allowed,
  Set<String> required,
) {
  if (value is! Map) {
    throw const FormatException('expected an object');
  }
  final map = <String, Object?>{};
  for (final MapEntry(:key, :value) in value.entries) {
    if (key is! String) {
      throw const FormatException('expected string keys');
    }
    if (!allowed.contains(key)) {
      throw FormatException('unknown field `$key`');
    }
    map[key] = value;
  }
  for (final key in required) {
    if (!map.containsKey(key)) {
      throw FormatException('missing field `$key`');
    }
  }
  return map;
}

String _requiredString(Map<String, Object?> map, String key) {
  final value = map[key];
  if (value is! String) {
    throw FormatException('invalid type for `$key`, expected a string');
  }
  return value;
}

String? _optionalString(Map<String, Object?> map, String key) {
  final value = map[key];
  if (value == null) {
    return null;
  }
  if (value is! String) {
    throw FormatException('invalid type for `$key`, expected a string');
  }
  return value;
}

int _requiredU32(Map<String, Object?> map, String key) {
  final value = map[key];
  if (value is! int || value < 0 || value > 0xFFFFFFFF) {
    throw FormatException('invalid value for `$key`, expected u32');
  }
  return value;
}

Object? _deepCopy(Object? value) {
  if (value is Map) {
    return <String, Object?>{
      for (final MapEntry(:key, :value) in value.entries)
        key as String: _deepCopy(value),
    };
  }
  if (value is List) {
    return [for (final item in value) _deepCopy(item)];
  }
  return value;
}

/// serde_json `value[key] = v`: an object gains/overwrites the key; `null`
/// becomes an object first. Other types panic in Rust; here they fail closed.
Object? _indexSet(Object? target, String key, Object? value) {
  if (target == null) {
    return <String, Object?>{key: value};
  }
  if (target is Map<String, Object?>) {
    target[key] = value;
    return target;
  }
  throw BackendError.validation('error.rfi.systemSync.approvedUpdateInvalid');
}

/// serde_json `Value` equality.
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
    for (var index = 0; index < a.length; index++) {
      if (!_jsonEquals(a[index], b[index])) {
        return false;
      }
    }
    return true;
  }
  if (a is num && b is num) {
    return (a is int) == (b is int) && a == b;
  }
  return a == b;
}
