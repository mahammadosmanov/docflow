/// Port of src-tauri/src/rfi_follow_up.rs: pure RFI Follow Up matching, layout
/// and classification primitives, plus the Plan/authority codecs.
library;
///
/// Interfaces owned by other engine modules and not yet present in lib/engine:
///  * OOXML read / package-signature / guarded patch / delta verification are
///    reached through [FollowUpOoxml]. The real implementation belongs to the
///    E6 ooxml port; the Follow Up logic here is complete and only needs that
///    adapter. UNVERIFIED until E6 supplies a conforming adapter.
///  * File identity (device/inode, Windows file index) comes from the caller
///    through [FollowUpFileIdentity]; Dart has no portable stat for it.
///    UNVERIFIED until the cap_fs / journal ports supply it.
///  * The rfi_log `status_columns()` table is mirrored as
///    [followUpStatusColumns]; replace with the rfi_log port when it lands.
import 'dart:collection';
import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import 'contracts.dart';
import 'errors.dart';
import 'ooxml.dart';
import 'rfi_code.dart';

typedef FollowUpJson = Map<String, Object?>;

const String followUpOperationId = 'rfi.follow_up_sync';
const int _sourceFirstDataRow = 10;
const int _maxSourceDataRowsPerSheet = 20000;
const int _maxHeaderRows = 32;
const int _maxHeaderColumns = 64;
const int _maxFollowUpDataRows = 20000;
const int _maxPreviewRows = 5000;
const int _maxSourceEvidencePerRfi = 500;
const int _maxPreviewAuthorityBytes = 32 * 1024 * 1024;
const int _maxFollowUpMergeComparisons = 20000000;
const int _maxCellTextChars = 20000;
const String _fullRfiPrefix = 'AZE-SUN-INT-QMS-RFI-';
const String _worleySignedMarker = 'Signed by Worley';

/// Test-visible counter mirroring the Rust `SOURCE_ANALYSIS_CALLS` cfg(test) static.
int followUpSourceAnalysisCalls = 0;

// ---------------------------------------------------------------------------
// Errors and interfaces
// ---------------------------------------------------------------------------

/// Error raised by an OOXML operation. Port of `OoxmlError` kinds as used here.
class FollowUpOoxmlError implements Exception {
  const FollowUpOoxmlError(this.limitExceeded, this.message);
  final bool limitExceeded;
  final String message;
  @override
  String toString() => message;
}

/// A parsed workbook: discovery facts plus worksheet data.
class FollowUpParsedWorkbook {
  const FollowUpParsedWorkbook(this.discovery, this.workbook);
  final OrderedWorkbookDiscovery discovery;
  final WorkbookData workbook;
}

/// Adapter over the OOXML package layer (E6). Mirrors the free functions
/// `read_workbook_and_discovery`, `package_has_signature`,
/// `patch_workbook_sheets_guarded_text` and `verify_guarded_workbook_delta_text`.
abstract interface class FollowUpOoxml {
  FollowUpParsedWorkbook readWorkbookAndDiscovery(Uint8List bytes);
  bool packageHasSignature(Uint8List bytes);
  Uint8List patchWorkbookSheetsGuardedText(
    Uint8List source,
    Map<String, Map<(int, int), GuardedCellUpdate>> updates,
  );
  void verifyGuardedWorkbookDeltaText(
    Uint8List original,
    Uint8List output,
    Map<String, Map<(int, int), GuardedCellUpdate>> updates,
  );
}

/// Stable identity of a filesystem object. Port of `FilesystemObjectIdentity`.
class FollowUpFileIdentity {
  const FollowUpFileIdentity.unix({required this.device, required this.inode})
    : kind = 'unix',
      volumeSerial = null,
      fileIndex = null;
  const FollowUpFileIdentity.windows({
    required this.volumeSerial,
    required this.fileIndex,
  }) : kind = 'windows',
       device = null,
       inode = null;

  final String kind;
  final int? device;
  final int? inode;
  final int? volumeSerial;
  final int? fileIndex;

  JsonObject toJson() => kind == 'unix'
      ? {'kind': 'unix', 'device': device, 'inode': inode}
      : {'kind': 'windows', 'volumeSerial': volumeSerial, 'fileIndex': fileIndex};

  factory FollowUpFileIdentity.fromJson(Object? value) {
    final map = _strictObject(
      value,
      allowed: const {'kind', 'device', 'inode', 'volumeSerial', 'fileIndex'},
      required: const {'kind'},
    );
    final kind = map['kind'];
    if (kind == 'unix') {
      return FollowUpFileIdentity.unix(
        device: _int(map, 'device'),
        inode: _int(map, 'inode'),
      );
    }
    if (kind == 'windows') {
      return FollowUpFileIdentity.windows(
        volumeSerial: _int(map, 'volumeSerial'),
        fileIndex: _int(map, 'fileIndex'),
      );
    }
    throw _workbookInvalidError();
  }

  @override
  bool operator ==(Object other) =>
      other is FollowUpFileIdentity &&
      other.kind == kind &&
      other.device == device &&
      other.inode == inode &&
      other.volumeSerial == volumeSerial &&
      other.fileIndex == fileIndex;

  @override
  int get hashCode => Object.hash(kind, device, inode, volumeSerial, fileIndex);
}

/// A workbook file as the caller opened it: path, bytes and filesystem facts.
class FollowUpWorkbookFile {
  const FollowUpWorkbookFile({
    required this.path,
    required this.bytes,
    required this.identity,
    required this.mtimeNs,
  });

  final String path;
  final Uint8List bytes;
  final FollowUpFileIdentity identity;
  final int mtimeNs;

  FollowUpWorkbookFile withPath(String value) => FollowUpWorkbookFile(
    path: value,
    bytes: bytes,
    identity: identity,
    mtimeNs: mtimeNs,
  );
}

// ---------------------------------------------------------------------------
// Fingerprint
// ---------------------------------------------------------------------------

/// Port of `WorkbookFingerprint`.
class FollowUpFingerprint {
  const FollowUpFingerprint({
    required this.identity,
    required this.size,
    required this.mtimeNs,
    required this.sha256,
  });

  final FollowUpFileIdentity identity;
  final int size;
  final int mtimeNs;
  final String sha256;

  JsonObject toJson() => {
    'identity': identity.toJson(),
    'size': size,
    'mtimeNs': mtimeNs,
    'sha256': sha256,
  };

  factory FollowUpFingerprint.fromJson(Object? value) {
    final map = _strictObject(
      value,
      allowed: const {'identity', 'size', 'mtimeNs', 'sha256'},
      required: const {'identity', 'size', 'mtimeNs', 'sha256'},
    );
    return FollowUpFingerprint(
      identity: FollowUpFileIdentity.fromJson(map['identity']),
      size: _int(map, 'size'),
      mtimeNs: _int(map, 'mtimeNs'),
      sha256: _str(map, 'sha256'),
    );
  }

  @override
  bool operator ==(Object other) =>
      other is FollowUpFingerprint &&
      other.identity == identity &&
      other.size == size &&
      other.mtimeNs == mtimeNs &&
      other.sha256 == sha256;

  @override
  int get hashCode => Object.hash(identity, size, mtimeNs, sha256);
}

FollowUpFingerprint fingerprintWorkbookFile(FollowUpWorkbookFile file) {
  return FollowUpFingerprint(
    identity: file.identity,
    size: file.bytes.length,
    mtimeNs: file.mtimeNs,
    sha256: sha256Hex(file.bytes),
  );
}

// ---------------------------------------------------------------------------
// Cell snapshots, layouts, rows
// ---------------------------------------------------------------------------

/// Port of `CellSnapshot` (serde camelCase, deny_unknown_fields). Uses the
/// OOXML [CellData] shape, which carries the same value/formula/style triple.
JsonObject cellSnapshotToJson(CellData cell) => {
  'value': cell.value,
  'formula': cell.formula,
  'style': cell.style,
};

CellData cellSnapshotFromJson(Object? value) {
  final map = _strictObject(
    value,
    allowed: const {'value', 'formula', 'style'},
    required: const {'value'},
  );
  return CellData(
    value: _str(map, 'value'),
    formula: _optStr(map, 'formula'),
    style: _optStr(map, 'style'),
  );
}

CellData _cellFromSnapshot(CellData cell) =>
    CellData(value: cell.value, formula: cell.formula, style: cell.style);

String visibilityWire(WorksheetVisibility visibility) => switch (visibility) {
  WorksheetVisibility.visible => 'visible',
  WorksheetVisibility.hidden => 'hidden',
  WorksheetVisibility.veryHidden => 'veryHidden',
};

WorksheetVisibility visibilityFromWire(Object? value) => switch (value) {
  'visible' => WorksheetVisibility.visible,
  'hidden' => WorksheetVisibility.hidden,
  'veryHidden' => WorksheetVisibility.veryHidden,
  _ => throw _workbookInvalidError(),
};

JsonObject worksheetDescriptorToJson(WorksheetDescriptor d) => {
  'workbookIndex': d.workbookIndex,
  'sheetId': d.sheetId,
  'relationshipId': d.relationshipId,
  'relationshipType': d.relationshipType,
  'partPath': d.partPath,
  'name': d.name,
  'visibility': visibilityWire(d.visibility),
};

WorksheetDescriptor worksheetDescriptorFromJson(Object? value) {
  final map = _strictObject(
    value,
    allowed: const {
      'workbookIndex',
      'sheetId',
      'relationshipId',
      'relationshipType',
      'partPath',
      'name',
      'visibility',
    },
    required: const {
      'workbookIndex',
      'sheetId',
      'relationshipId',
      'relationshipType',
      'partPath',
      'name',
      'visibility',
    },
  );
  return WorksheetDescriptor(
    workbookIndex: _int(map, 'workbookIndex'),
    sheetId: _int(map, 'sheetId'),
    relationshipId: _str(map, 'relationshipId'),
    relationshipType: _str(map, 'relationshipType'),
    partPath: _str(map, 'partPath'),
    name: _str(map, 'name'),
    visibility: visibilityFromWire(map['visibility']),
  );
}

/// Port of `FollowUpLayout`.
class FollowUpLayout {
  const FollowUpLayout({
    required this.worksheet,
    required this.headerRow,
    required this.rfiColumn,
    required this.statusColumn,
    this.clause,
  });

  final WorksheetDescriptor worksheet;
  final int headerRow;
  final int rfiColumn;
  final int statusColumn;
  final String? clause;

  JsonObject toJson() => {
    'worksheet': worksheetDescriptorToJson(worksheet),
    'headerRow': headerRow,
    'rfiColumn': rfiColumn,
    'statusColumn': statusColumn,
    if (clause != null) 'clause': clause,
  };

  factory FollowUpLayout.fromJson(Object? value) {
    final map = _strictObject(
      value,
      allowed: const {
        'worksheet',
        'headerRow',
        'rfiColumn',
        'statusColumn',
        'clause',
      },
      required: const {'worksheet', 'headerRow', 'rfiColumn', 'statusColumn'},
    );
    return FollowUpLayout(
      worksheet: worksheetDescriptorFromJson(map['worksheet']),
      headerRow: _int(map, 'headerRow'),
      rfiColumn: _int(map, 'rfiColumn'),
      statusColumn: _int(map, 'statusColumn'),
      clause: _optStr(map, 'clause'),
    );
  }

  @override
  bool operator ==(Object other) =>
      other is FollowUpLayout &&
      other.worksheet == worksheet &&
      other.headerRow == headerRow &&
      other.rfiColumn == rfiColumn &&
      other.statusColumn == statusColumn &&
      other.clause == clause;

  @override
  int get hashCode =>
      Object.hash(worksheet, headerRow, rfiColumn, statusColumn, clause);
}

/// Port of `FollowUpDisposition`.
enum FollowUpDisposition {
  ready('rfi.followUp.disposition.ready'),
  alreadySynced('rfi.followUp.disposition.alreadySynced'),
  manualReview('rfi.followUp.disposition.manualReview');

  const FollowUpDisposition(this.wire);
  final String wire;

  static FollowUpDisposition fromWire(Object? value) {
    for (final item in values) {
      if (item.wire == value) return item;
    }
    throw _workbookInvalidError();
  }
}

/// Port of `FollowUpReviewTarget`.
class FollowUpReviewTarget {
  const FollowUpReviewTarget({
    required this.worksheet,
    required this.row,
    required this.rfiColumn,
    required this.statusColumn,
    this.rfiCell,
    this.statusCell,
  });

  final WorksheetDescriptor worksheet;
  final int row;
  final int rfiColumn;
  final int statusColumn;
  final CellData? rfiCell;
  final CellData? statusCell;

  JsonObject toJson() => {
    'worksheet': worksheetDescriptorToJson(worksheet),
    'row': row,
    'rfiColumn': rfiColumn,
    'statusColumn': statusColumn,
    'rfiCell': rfiCell == null ? null : cellSnapshotToJson(rfiCell!),
    'statusCell': statusCell == null ? null : cellSnapshotToJson(statusCell!),
  };

  factory FollowUpReviewTarget.fromJson(Object? value) {
    final map = _strictObject(
      value,
      allowed: const {
        'worksheet',
        'row',
        'rfiColumn',
        'statusColumn',
        'rfiCell',
        'statusCell',
      },
      required: const {'worksheet', 'row', 'rfiColumn', 'statusColumn'},
    );
    return FollowUpReviewTarget(
      worksheet: worksheetDescriptorFromJson(map['worksheet']),
      row: _int(map, 'row'),
      rfiColumn: _int(map, 'rfiColumn'),
      statusColumn: _int(map, 'statusColumn'),
      rfiCell: map['rfiCell'] == null
          ? null
          : cellSnapshotFromJson(map['rfiCell']),
      statusCell: map['statusCell'] == null
          ? null
          : cellSnapshotFromJson(map['statusCell']),
    );
  }

  @override
  bool operator ==(Object other) =>
      other is FollowUpReviewTarget &&
      other.worksheet == worksheet &&
      other.row == row &&
      other.rfiColumn == rfiColumn &&
      other.statusColumn == statusColumn &&
      other.rfiCell == rfiCell &&
      other.statusCell == statusCell;

  @override
  int get hashCode => Object.hash(
    worksheet,
    row,
    rfiColumn,
    statusColumn,
    rfiCell,
    statusCell,
  );
}

/// Port of `SourceStatusEvidence`.
class FollowUpSourceEvidence {
  const FollowUpSourceEvidence({
    required this.sheet,
    required this.row,
    required this.rfiColumn,
    required this.statusColumn,
    required this.rfiCell,
    required this.statusCell,
  });

  final String sheet;
  final int row;
  final int rfiColumn;
  final int statusColumn;
  final CellData rfiCell;
  final CellData? statusCell;

  JsonObject toJson() => {
    'sheet': sheet,
    'row': row,
    'rfiColumn': rfiColumn,
    'statusColumn': statusColumn,
    'rfiCell': cellSnapshotToJson(rfiCell),
    'statusCell': statusCell == null ? null : cellSnapshotToJson(statusCell!),
  };

  factory FollowUpSourceEvidence.fromJson(Object? value) {
    final map = _strictObject(
      value,
      allowed: const {
        'sheet',
        'row',
        'rfiColumn',
        'statusColumn',
        'rfiCell',
        'statusCell',
      },
      required: const {'sheet', 'row', 'rfiColumn', 'statusColumn', 'rfiCell'},
    );
    return FollowUpSourceEvidence(
      sheet: _str(map, 'sheet'),
      row: _int(map, 'row'),
      rfiColumn: _int(map, 'rfiColumn'),
      statusColumn: _int(map, 'statusColumn'),
      rfiCell: cellSnapshotFromJson(map['rfiCell']),
      statusCell: map['statusCell'] == null
          ? null
          : cellSnapshotFromJson(map['statusCell']),
    );
  }

  @override
  bool operator ==(Object other) =>
      other is FollowUpSourceEvidence &&
      other.sheet == sheet &&
      other.row == row &&
      other.rfiColumn == rfiColumn &&
      other.statusColumn == statusColumn &&
      other.rfiCell == rfiCell &&
      other.statusCell == statusCell;

  @override
  int get hashCode =>
      Object.hash(sheet, row, rfiColumn, statusColumn, rfiCell, statusCell);
}

/// Port of `FollowUpRow` (serde camelCase, deny_unknown_fields). Editor rows
/// carry `_rowId`, `_sheet` and `_row` keys.
class FollowUpRow {
  const FollowUpRow({
    required this.rowId,
    required this.sheet,
    required this.row,
    required this.rfiNo,
    required this.sourceStatuses,
    required this.currentStatus,
    required this.proposedStatus,
    required this.proposedRfiNo,
    required this.disposition,
    required this.reasonKey,
    required this.selectable,
    required this.enabled,
    required this.target,
    required this.sourceEvidence,
    required this.mappingDetails,
  });

  final String rowId;
  final String sheet;
  final int row;
  final String rfiNo;
  final List<String> sourceStatuses;
  final String currentStatus;
  final String? proposedStatus;
  final String? proposedRfiNo;
  final FollowUpDisposition disposition;
  final String reasonKey;
  final bool selectable;
  final bool enabled;
  final FollowUpReviewTarget target;
  final List<FollowUpSourceEvidence> sourceEvidence;
  final String? mappingDetails;

  FollowUpRow copyWith({bool? enabled}) => FollowUpRow(
    rowId: rowId,
    sheet: sheet,
    row: row,
    rfiNo: rfiNo,
    sourceStatuses: sourceStatuses,
    currentStatus: currentStatus,
    proposedStatus: proposedStatus,
    proposedRfiNo: proposedRfiNo,
    disposition: disposition,
    reasonKey: reasonKey,
    selectable: selectable,
    enabled: enabled ?? this.enabled,
    target: target,
    sourceEvidence: sourceEvidence,
    mappingDetails: mappingDetails,
  );

  JsonObject toJson() => {
    '_rowId': rowId,
    '_sheet': sheet,
    '_row': row,
    'rfiNo': rfiNo,
    'sourceStatuses': sourceStatuses,
    'currentStatus': currentStatus,
    'proposedStatus': proposedStatus,
    'proposedRfiNo': proposedRfiNo,
    'disposition': disposition.wire,
    'reasonKey': reasonKey,
    'selectable': selectable,
    'enabled': enabled,
    'target': target.toJson(),
    'sourceEvidence': [for (final e in sourceEvidence) e.toJson()],
    if (mappingDetails != null) 'mappingDetails': mappingDetails,
  };

  factory FollowUpRow.fromJson(Object? value) {
    final map = _strictObject(
      value,
      allowed: const {
        '_rowId',
        '_sheet',
        '_row',
        'rfiNo',
        'sourceStatuses',
        'currentStatus',
        'proposedStatus',
        'proposedRfiNo',
        'disposition',
        'reasonKey',
        'selectable',
        'enabled',
        'target',
        'sourceEvidence',
        'mappingDetails',
      },
      required: const {
        '_rowId',
        '_sheet',
        '_row',
        'rfiNo',
        'sourceStatuses',
        'currentStatus',
        'disposition',
        'reasonKey',
        'selectable',
        'enabled',
        'target',
        'sourceEvidence',
      },
    );
    return FollowUpRow(
      rowId: _str(map, '_rowId'),
      sheet: _str(map, '_sheet'),
      row: _int(map, '_row'),
      rfiNo: _str(map, 'rfiNo'),
      sourceStatuses: _strList(map, 'sourceStatuses'),
      currentStatus: _str(map, 'currentStatus'),
      proposedStatus: _optStr(map, 'proposedStatus'),
      proposedRfiNo: _optStr(map, 'proposedRfiNo'),
      disposition: FollowUpDisposition.fromWire(map['disposition']),
      reasonKey: _str(map, 'reasonKey'),
      selectable: _bool(map, 'selectable'),
      enabled: _bool(map, 'enabled'),
      target: FollowUpReviewTarget.fromJson(map['target']),
      sourceEvidence: [
        for (final item in _list(map, 'sourceEvidence'))
          FollowUpSourceEvidence.fromJson(item),
      ],
      mappingDetails: _optStr(map, 'mappingDetails'),
    );
  }

  @override
  bool operator ==(Object other) => other is FollowUpRow && _jsonEquals(toJson(), other.toJson());

  @override
  int get hashCode => rowId.hashCode;
}

/// Port of `FollowUpPlanMeta`, stored under `plan.meta["rfiFollowUp"]`.
class FollowUpPlanMeta {
  const FollowUpPlanMeta({
    required this.rfiLogWorkbook,
    required this.followUpWorkbook,
    required this.backupWorkbook,
    required this.rfiLogFingerprint,
    required this.followUpFingerprint,
    required this.layout,
    required this.layouts,
  });

  final String rfiLogWorkbook;
  final String followUpWorkbook;
  final String backupWorkbook;
  final FollowUpFingerprint rfiLogFingerprint;
  final FollowUpFingerprint followUpFingerprint;
  final FollowUpLayout layout;
  final List<FollowUpLayout> layouts;

  JsonObject toJson() => {
    'rfiLogWorkbook': rfiLogWorkbook,
    'followUpWorkbook': followUpWorkbook,
    'backupWorkbook': backupWorkbook,
    'rfiLogFingerprint': rfiLogFingerprint.toJson(),
    'followUpFingerprint': followUpFingerprint.toJson(),
    'layout': layout.toJson(),
    'layouts': [for (final item in layouts) item.toJson()],
  };

  factory FollowUpPlanMeta.fromJson(Object? value) {
    final map = _strictObject(
      value,
      allowed: const {
        'rfiLogWorkbook',
        'followUpWorkbook',
        'backupWorkbook',
        'rfiLogFingerprint',
        'followUpFingerprint',
        'layout',
        'layouts',
      },
      required: const {
        'rfiLogWorkbook',
        'followUpWorkbook',
        'backupWorkbook',
        'rfiLogFingerprint',
        'followUpFingerprint',
        'layout',
      },
    );
    return FollowUpPlanMeta(
      rfiLogWorkbook: _str(map, 'rfiLogWorkbook'),
      followUpWorkbook: _str(map, 'followUpWorkbook'),
      backupWorkbook: _str(map, 'backupWorkbook'),
      rfiLogFingerprint: FollowUpFingerprint.fromJson(map['rfiLogFingerprint']),
      followUpFingerprint: FollowUpFingerprint.fromJson(
        map['followUpFingerprint'],
      ),
      layout: FollowUpLayout.fromJson(map['layout']),
      layouts: [
        for (final item in _listOrEmpty(map, 'layouts'))
          FollowUpLayout.fromJson(item),
      ],
    );
  }

  @override
  bool operator ==(Object other) =>
      other is FollowUpPlanMeta && _jsonEquals(toJson(), other.toJson());

  @override
  int get hashCode => rfiLogWorkbook.hashCode;
}

/// Port of `FollowUpCellTarget` inside an approved update.
class FollowUpCellTarget {
  const FollowUpCellTarget({
    required this.worksheet,
    required this.row,
    required this.rfiColumn,
    required this.statusColumn,
    required this.rfiCell,
    required this.statusCell,
  });

  final WorksheetDescriptor worksheet;
  final int row;
  final int rfiColumn;
  final int statusColumn;
  final CellData rfiCell;
  final CellData statusCell;

  JsonObject toJson() => {
    'worksheet': worksheetDescriptorToJson(worksheet),
    'row': row,
    'rfiColumn': rfiColumn,
    'statusColumn': statusColumn,
    'rfiCell': cellSnapshotToJson(rfiCell),
    'statusCell': cellSnapshotToJson(statusCell),
  };

  factory FollowUpCellTarget.fromJson(Object? value) {
    final map = _strictObject(
      value,
      allowed: const {
        'worksheet',
        'row',
        'rfiColumn',
        'statusColumn',
        'rfiCell',
        'statusCell',
      },
      required: const {
        'worksheet',
        'row',
        'rfiColumn',
        'statusColumn',
        'rfiCell',
        'statusCell',
      },
    );
    return FollowUpCellTarget(
      worksheet: worksheetDescriptorFromJson(map['worksheet']),
      row: _int(map, 'row'),
      rfiColumn: _int(map, 'rfiColumn'),
      statusColumn: _int(map, 'statusColumn'),
      rfiCell: cellSnapshotFromJson(map['rfiCell']),
      statusCell: cellSnapshotFromJson(map['statusCell']),
    );
  }

  @override
  bool operator ==(Object other) =>
      other is FollowUpCellTarget && _jsonEquals(toJson(), other.toJson());

  @override
  int get hashCode => row.hashCode;
}

/// Port of `FollowUpApprovedUpdate`.
class FollowUpApprovedUpdate {
  const FollowUpApprovedUpdate({
    required this.rowId,
    required this.canonicalRfi,
    required this.proposedStatus,
    required this.proposedRfi,
    required this.target,
    required this.sourceEvidence,
  });

  final String rowId;
  final String canonicalRfi;
  final String proposedStatus;
  final String proposedRfi;
  final FollowUpCellTarget target;
  final List<FollowUpSourceEvidence> sourceEvidence;

  JsonObject toJson() => {
    'rowId': rowId,
    'canonicalRfi': canonicalRfi,
    'proposedStatus': proposedStatus,
    'proposedRfi': proposedRfi,
    'target': target.toJson(),
    'sourceEvidence': [for (final e in sourceEvidence) e.toJson()],
  };

  factory FollowUpApprovedUpdate.fromJson(Object? value) {
    final map = _strictObject(
      value,
      allowed: const {
        'rowId',
        'canonicalRfi',
        'proposedStatus',
        'proposedRfi',
        'target',
        'sourceEvidence',
      },
      required: const {
        'rowId',
        'canonicalRfi',
        'proposedStatus',
        'target',
        'sourceEvidence',
      },
    );
    return FollowUpApprovedUpdate(
      rowId: _str(map, 'rowId'),
      canonicalRfi: _str(map, 'canonicalRfi'),
      proposedStatus: _str(map, 'proposedStatus'),
      proposedRfi: _optStr(map, 'proposedRfi') ?? '',
      target: FollowUpCellTarget.fromJson(map['target']),
      sourceEvidence: [
        for (final item in _list(map, 'sourceEvidence'))
          FollowUpSourceEvidence.fromJson(item),
      ],
    );
  }

  @override
  bool operator ==(Object other) =>
      other is FollowUpApprovedUpdate && _jsonEquals(toJson(), other.toJson());

  @override
  int get hashCode => rowId.hashCode;
}

/// Port of `FollowUpStepMeta`, the metadata of the single WorkbookUpdate step.
class FollowUpStepMeta {
  const FollowUpStepMeta({
    required this.rfiLogWorkbook,
    required this.rfiLogFingerprint,
    required this.followUpFingerprint,
    required this.layout,
    required this.layouts,
    required this.updates,
  });

  final String rfiLogWorkbook;
  final FollowUpFingerprint rfiLogFingerprint;
  final FollowUpFingerprint followUpFingerprint;
  final FollowUpLayout layout;
  final List<FollowUpLayout> layouts;
  final List<FollowUpApprovedUpdate> updates;

  /// Port of `all_layouts`.
  Iterable<FollowUpLayout> get allLayouts =>
      layouts.isEmpty ? [layout] : layouts;

  JsonObject toJson() => {
    'rfiLogWorkbook': rfiLogWorkbook,
    'rfiLogFingerprint': rfiLogFingerprint.toJson(),
    'followUpFingerprint': followUpFingerprint.toJson(),
    'layout': layout.toJson(),
    'layouts': [for (final item in layouts) item.toJson()],
    'updates': [for (final item in updates) item.toJson()],
  };

  factory FollowUpStepMeta.fromJson(JsonObject value) {
    final map = _strictObject(
      value,
      allowed: const {
        'rfiLogWorkbook',
        'rfiLogFingerprint',
        'followUpFingerprint',
        'layout',
        'layouts',
        'updates',
      },
      required: const {
        'rfiLogWorkbook',
        'rfiLogFingerprint',
        'followUpFingerprint',
        'layout',
        'updates',
      },
    );
    return FollowUpStepMeta(
      rfiLogWorkbook: _str(map, 'rfiLogWorkbook'),
      rfiLogFingerprint: FollowUpFingerprint.fromJson(map['rfiLogFingerprint']),
      followUpFingerprint: FollowUpFingerprint.fromJson(
        map['followUpFingerprint'],
      ),
      layout: FollowUpLayout.fromJson(map['layout']),
      layouts: [
        for (final item in _listOrEmpty(map, 'layouts'))
          FollowUpLayout.fromJson(item),
      ],
      updates: [
        for (final item in _list(map, 'updates'))
          FollowUpApprovedUpdate.fromJson(item),
      ],
    );
  }
}

/// Port of `GuardedCellUpdate` for the Follow Up text replacements.
GuardedCellUpdate followUpTextUpdate(CellData expected, String replacement) =>
    GuardedCellUpdate(
      expected: expected,
      replacement: CellUpdateText(replacement),
      createIfAbsent: false,
    );

// ---------------------------------------------------------------------------
// Plan / Step / classification projection (JSON shapes match the Rust serde)
// ---------------------------------------------------------------------------

/// Columns of the editor, port of `editor_columns`.
List<JsonObject> editorColumns() => [
  {
    'key': 'enabled',
    'labelKey': 'rfi.followUp.column.enabled',
    'kind': 'bool',
    'bulk': false,
    'disabledWhen': {'field': 'selectable', 'equals': false},
  },
  {'key': 'rfiNo', 'labelKey': 'rfi.followUp.column.rfiNo', 'kind': 'readonly'},
  {
    'key': 'sourceStatuses',
    'labelKey': 'rfi.followUp.column.sourceStatuses',
    'kind': 'readonly',
  },
  {
    'key': 'currentStatus',
    'labelKey': 'rfi.followUp.column.currentStatus',
    'kind': 'readonly',
  },
  {
    'key': 'proposedRfiNo',
    'labelKey': 'rfi.followUp.column.proposedRfiNo',
    'kind': 'readonly',
  },
  {
    'key': 'proposedStatus',
    'labelKey': 'rfi.followUp.column.proposedStatus',
    'kind': 'readonly',
  },
  {
    'key': 'disposition',
    'labelKey': 'rfi.followUp.column.disposition',
    'kind': 'readonly',
    'display': 'localizedKey',
  },
  {
    'key': 'reasonKey',
    'labelKey': 'rfi.followUp.column.reason',
    'kind': 'readonly',
    'display': 'localizedKey',
    'detail': true,
  },
  {
    'key': 'mappingDetails',
    'labelKey': 'rfi.followUp.column.mappingDetails',
    'kind': 'readonly',
    'width': 320,
    'detail': true,
  },
  {
    'key': '_sheet',
    'labelKey': 'rfi.followUp.column.sheet',
    'kind': 'readonly',
    'detail': true,
  },
  {
    'key': '_row',
    'labelKey': 'rfi.followUp.column.row',
    'kind': 'readonly',
    'detail': true,
  },
];

/// Port of `editor_columns_legacy`.
List<JsonObject> editorColumnsLegacy() => [
  for (final column in editorColumns())
    if (column['key'] != 'proposedRfiNo' && column['key'] != 'mappingDetails')
      column,
];

/// Port of `editor_metadata`.
JsonObject editorMetadata(List<FollowUpRow> rows, List<JsonObject> columns) => {
  'kind': 'recordTable',
  'layout': 'compact',
  'titleKey': 'rfi.followUp.editor.title',
  'rowCountKey': 'rfi.followUp.editor.rowCount',
  'requiredFields': ['enabled'],
  'groupField': 'rfiNo',
  'selectionGroupFields': <String>[],
  'columns': columns,
  'rows': [for (final row in rows) row.toJson()],
  'duplicateEnabled': false,
  'selectionEnabled': false,
  'bulkEnabled': false,
  'reorderable': false,
  'detailsEnabled': true,
  'detailsDefaultVisible': rows.any((row) => row.mappingDetails != null),
  'copyEnabled': false,
};

/// Port of `plan_from_classification`. Accepts the classification projection
/// (`layout`, optional `layouts`, `rows`, `warnings`, `editor`).
Plan planFromClassification({
  required String rfiLogWorkbook,
  required String followUpWorkbook,
  required String backupWorkbook,
  required FollowUpFingerprint rfiLogFingerprint,
  required FollowUpFingerprint followUpFingerprint,
  required JsonObject classification,
}) {
  final layout = FollowUpLayout.fromJson(_requiredField(classification, 'layout'));
  List<FollowUpLayout> layouts;
  final rawLayouts = classification['layouts'];
  if (rawLayouts == null) {
    layouts = [layout];
  } else {
    try {
      layouts = [
        for (final item in rawLayouts as List) FollowUpLayout.fromJson(item),
      ];
    } catch (_) {
      layouts = [layout];
    }
  }
  final rows = [
    for (final item in _requiredList(classification, 'rows'))
      FollowUpRow.fromJson(item),
  ];
  final warnings = [
    for (final item in _requiredList(classification, 'warnings'))
      Warning.fromJson(_asJsonObject(item)),
  ];
  final editor = _requiredField(classification, 'editor');

  final updates = <FollowUpApprovedUpdate>[];
  for (final row in rows.where((row) => row.enabled)) {
    if (!row.selectable || row.disposition != FollowUpDisposition.ready) {
      throw _workbookInvalidError();
    }
    final canonical = canonicalFollowUpRfi(row.rfiNo);
    if (canonical == null) throw _workbookInvalidError();
    final proposedStatus = row.proposedStatus;
    if (proposedStatus == null) throw _workbookInvalidError();
    final target = row.target;
    final rfiCell = target.rfiCell;
    final statusCell = target.statusCell;
    if (rfiCell == null || statusCell == null) throw _workbookInvalidError();
    updates.add(
      FollowUpApprovedUpdate(
        rowId: row.rowId,
        canonicalRfi: canonical.$1,
        proposedStatus: proposedStatus,
        proposedRfi: row.proposedRfiNo ?? rfiCell.value,
        target: FollowUpCellTarget(
          worksheet: target.worksheet,
          row: target.row,
          rfiColumn: target.rfiColumn,
          statusColumn: target.statusColumn,
          rfiCell: rfiCell,
          statusCell: statusCell,
        ),
        sourceEvidence: row.sourceEvidence,
      ),
    );
  }

  final planMeta = FollowUpPlanMeta(
    rfiLogWorkbook: rfiLogWorkbook,
    followUpWorkbook: followUpWorkbook,
    backupWorkbook: backupWorkbook,
    rfiLogFingerprint: rfiLogFingerprint,
    followUpFingerprint: followUpFingerprint,
    layout: layout,
    layouts: layouts,
  );
  final meta = <String, Object?>{
    'editor': editor,
    'rfiFollowUp': planMeta.toJson(),
  };
  final steps = <Step>[];
  if (updates.isNotEmpty) {
    final stepMeta = FollowUpStepMeta(
      rfiLogWorkbook: rfiLogWorkbook,
      rfiLogFingerprint: rfiLogFingerprint,
      followUpFingerprint: followUpFingerprint,
      layout: layout,
      layouts: layouts,
      updates: updates,
    ).toJson();
    steps.add(
      Step(
        kind: StepKind.workbookUpdate,
        src: followUpWorkbook,
        dst: backupWorkbook,
        noteKey: 'note.rfi.followUp.workbookUpdate',
        meta: stepMeta,
      ),
    );
  }
  return Plan(
    planId: '',
    opId: followUpOperationId,
    steps: steps,
    warnings: warnings,
    meta: meta,
  );
}

/// Port of `decode_step_meta`: strict decode plus every authority check.
FollowUpStepMeta decodeStepMeta(Step step) {
  if (step.kind != StepKind.workbookUpdate) throw _workbookInvalidError();
  final FollowUpStepMeta meta;
  try {
    meta = FollowUpStepMeta.fromJson(step.meta);
  } on BackendError {
    rethrow;
  } catch (_) {
    throw _workbookInvalidError();
  }
  if (meta.updates.isEmpty || meta.updates.length > _maxPreviewRows) {
    throw _workbookInvalidError();
  }
  final rowIds = <String>{};
  final targets = <(String, int, int)>{};
  for (final update in meta.updates) {
    final validLayout = meta.allLayouts.any(
      (layout) =>
          layout.worksheet == update.target.worksheet &&
          layout.rfiColumn == update.target.rfiColumn &&
          layout.statusColumn == update.target.statusColumn,
    );
    if (!validLayout ||
        !rowIds.add(update.rowId) ||
        !targets.add((
          update.target.worksheet.partPath,
          update.target.row,
          update.target.statusColumn,
        )) ||
        canonicalRfi(update.canonicalRfi) != update.canonicalRfi ||
        !const {
          '',
          'Pending',
          'In Progress',
          'Completed',
          'Signed',
          'Not Signed',
        }.contains(update.proposedStatus) ||
        (update.proposedRfi.isNotEmpty &&
            parseRfiGroup(update.proposedRfi) == null) ||
        (rustTrim(update.target.rfiCell.value).isNotEmpty &&
            parseRfiGroup(update.target.rfiCell.value) == null) ||
        update.target.rfiCell.formula != null ||
        update.target.statusCell.formula != null) {
      throw _workbookInvalidError();
    }
  }
  return meta;
}

/// Port of `guarded_updates`: sheet name -> (row, column) -> guarded update.
Map<String, Map<(int, int), GuardedCellUpdate>> guardedUpdates(Step step) {
  final meta = decodeStepMeta(step);
  final updatesBySheet = SplayTreeMap<String, Map<(int, int), GuardedCellUpdate>>(
    compareCodePoints,
  );
  for (final update in meta.updates) {
    final sheetUpdates = updatesBySheet.putIfAbsent(
      update.target.worksheet.name,
      () => SplayTreeMap<(int, int), GuardedCellUpdate>(compareRowColumn),
    );
    if (update.proposedRfi != update.target.rfiCell.value) {
      final rfiGuarded = followUpTextUpdate(
        _cellFromSnapshot(update.target.rfiCell),
        update.proposedRfi,
      );
      final key = (update.target.row, update.target.rfiColumn);
      final existing = sheetUpdates[key];
      if (existing != null) {
        if (existing != rfiGuarded) throw _workbookInvalidError();
      } else {
        sheetUpdates[key] = rfiGuarded;
      }
    }
    final statusGuarded = followUpTextUpdate(
      _cellFromSnapshot(update.target.statusCell),
      update.proposedStatus,
    );
    final statusKey = (update.target.row, update.target.statusColumn);
    if (sheetUpdates.containsKey(statusKey)) throw _workbookInvalidError();
    sheetUpdates[statusKey] = statusGuarded;
  }
  return updatesBySheet;
}

/// Port of `changed_cell_count`.
int changedCellCount(Step step) {
  var total = 0;
  for (final sheet in guardedUpdates(step).values) {
    total += sheet.length;
  }
  return total;
}

/// Port of `step_workbook_paths`.
(String rfiLogWorkbook, String target, String backup) stepWorkbookPaths(
  Step step,
) {
  final meta = decodeStepMeta(step);
  final target = step.src ?? (throw _workbookInvalidError());
  final backup = step.dst ?? (throw _workbookInvalidError());
  return (meta.rfiLogWorkbook, target, backup);
}

/// Port of `revalidate_open_workbooks`.
void revalidateOpenWorkbooks(
  Step step,
  FollowUpWorkbookFile rfiLog,
  FollowUpWorkbookFile followUp,
) {
  final meta = decodeStepMeta(step);
  final source = fingerprintWorkbookFile(rfiLog);
  final target = fingerprintWorkbookFile(followUp);
  if (source != meta.rfiLogFingerprint || target != meta.followUpFingerprint) {
    throw BackendError.validation('error.rfi.followUp.workbookChanged');
  }
}

/// Port of `verify_patched_output`.
void verifyPatchedOutput(
  Step step,
  FollowUpOoxml ooxml,
  Uint8List source,
  Uint8List privateOutput,
) {
  final updates = guardedUpdates(step);
  try {
    ooxml.verifyGuardedWorkbookDeltaText(source, privateOutput, updates);
  } on FollowUpOoxmlError catch (error) {
    throw error.limitExceeded
        ? _limitExceededError()
        : BackendError.validation('error.rfi.followUp.patchVerification');
  }
}

/// Port of `write_and_verify_private_output`: patch then verify the delta.
Uint8List writeAndVerifyPrivateOutput(
  Step step,
  FollowUpOoxml ooxml,
  Uint8List source,
) {
  final updates = guardedUpdates(step);
  final Uint8List output;
  try {
    output = ooxml.patchWorkbookSheetsGuardedText(source, updates);
  } on FollowUpOoxmlError catch (error) {
    throw mapFollowUpOoxmlError(error, apply: true);
  }
  verifyPatchedOutput(step, ooxml, source, output);
  return output;
}

/// Port of `plan_workbook_paths`.
(String rfiLogWorkbook, String followUpWorkbook) planWorkbookPaths(Plan plan) {
  final authority = FollowUpPlanMeta.fromJson(
    _requiredField(plan.meta, 'rfiFollowUp'),
  );
  return (authority.rfiLogWorkbook, authority.followUpWorkbook);
}

/// Port of `validate_owned_plan_budget`.
void validateOwnedPlanBudget(Plan plan) {
  _validateJsonBudget(plan.toJson());
}

/// Maps an OOXML failure to the Follow Up backend error. Port of
/// `map_follow_up_ooxml_error` / `map_apply_ooxml_error`.
BackendError mapFollowUpOoxmlError(
  FollowUpOoxmlError error, {
  bool apply = false,
}) {
  if (error.limitExceeded) {
    return BackendError.validation(
      'error.rfi.followUp.limitExceeded: ${error.message}',
    );
  }
  return BackendError.validation(
    'error.rfi.followUp.workbookInvalid: ${error.message}',
  );
}

// ---------------------------------------------------------------------------
// Preview / revise
// ---------------------------------------------------------------------------

/// Port of `preview_from_open_files`. The Follow Up backup name uses a random
/// 32-hex token in place of the Rust `Uuid::new_v4().simple()`.
Plan previewFromOpenFiles({
  required FollowUpWorkbookFile rfiLog,
  required FollowUpWorkbookFile followUp,
  required FollowUpOoxml ooxml,
  String Function()? newToken,
}) {
  final sourceFingerprint = fingerprintWorkbookFile(rfiLog);
  final targetFingerprint = fingerprintWorkbookFile(followUp);
  if (sourceFingerprint.identity == targetFingerprint.identity) {
    throw BackendError.validation(
      'RFI Log and Follow Up workbooks must be different files',
    );
  }
  final sourceParsed = _readParsed(ooxml, rfiLog.bytes);
  final hasSignature = _packageHasSignature(ooxml, followUp.bytes);
  final targetParsed = _readParsed(ooxml, followUp.bytes);
  final classification = classifyWorkbooks(
    sourceParsed.discovery,
    sourceParsed.workbook,
    targetParsed.discovery,
    targetParsed.workbook,
    targetHasSignature: hasSignature,
  );
  final extension = _fileExtension(followUp.path);
  if (extension == null ||
      (extension.toLowerCase() != 'xlsx' && extension.toLowerCase() != 'xlsm')) {
    throw BackendError.validation(
      'error.rfi.followUp.workbookInvalid: Follow Up workbook must have .xlsx or .xlsm extension',
    );
  }
  final token = (newToken ?? _randomHex32)();
  final backup = _joinSibling(
    followUp.path,
    '.docflow-s5-backup-$token.${extension.toLowerCase()}',
  );
  return planFromClassification(
    rfiLogWorkbook: rfiLog.path,
    followUpWorkbook: followUp.path,
    backupWorkbook: backup,
    rfiLogFingerprint: sourceFingerprint,
    followUpFingerprint: targetFingerprint,
    classification: classification,
  );
}

/// Port of `revise_from_open_files`.
Plan reviseFromOpenFiles({
  required Plan plan,
  required List<JsonObject> submittedRows,
  required FollowUpWorkbookFile rfiLog,
  required FollowUpWorkbookFile followUp,
  required FollowUpOoxml ooxml,
  String Function()? newToken,
}) {
  if (plan.opId != followUpOperationId) {
    throw BackendError.validation('plan is not an RFI Follow Up plan');
  }
  final authority = FollowUpPlanMeta.fromJson(
    _requiredField(plan.meta, 'rfiFollowUp'),
  );
  final originalRows = [
    for (final item in _requiredEditorRows(plan.meta))
      FollowUpRow.fromJson(item),
  ];
  final revisedRows = <FollowUpRow>[
    for (final row in submittedRows) _decodeOrInvalid(() => FollowUpRow.fromJson(row)),
  ];
  if (originalRows.length != revisedRows.length) throw _workbookInvalidError();
  for (var i = 0; i < originalRows.length; i++) {
    final original = originalRows[i];
    final revised = revisedRows[i];
    if (original.rowId != revised.rowId ||
        !_rowsEqualExceptEnabled(original, revised)) {
      throw _workbookInvalidError();
    }
    if (revised.enabled &&
        (!original.selectable || original.disposition != FollowUpDisposition.ready)) {
      throw _workbookInvalidError();
    }
  }

  final fresh = previewFromOpenFiles(
    rfiLog: rfiLog.withPath(authority.rfiLogWorkbook),
    followUp: followUp.withPath(authority.followUpWorkbook),
    ooxml: ooxml,
    newToken: newToken,
  );
  final freshAuthority = FollowUpPlanMeta.fromJson(
    _requiredField(fresh.meta, 'rfiFollowUp'),
  );
  if (authority.rfiLogWorkbook != freshAuthority.rfiLogWorkbook ||
      authority.followUpWorkbook != freshAuthority.followUpWorkbook ||
      authority.rfiLogFingerprint != freshAuthority.rfiLogFingerprint ||
      authority.followUpFingerprint != freshAuthority.followUpFingerprint ||
      authority.layout != freshAuthority.layout ||
      !_layoutListEquals(authority.layouts, freshAuthority.layouts)) {
    throw BackendError.validation('error.rfi.followUp.workbookChanged');
  }
  final freshRows = [
    for (final item in _requiredEditorRows(fresh.meta))
      FollowUpRow.fromJson(item),
  ];
  if (freshRows.length != originalRows.length) {
    throw BackendError.validation('error.rfi.followUp.workbookChanged');
  }
  for (var i = 0; i < originalRows.length; i++) {
    if (!_rowsEqualExceptEnabled(originalRows[i], freshRows[i])) {
      throw BackendError.validation('error.rfi.followUp.workbookChanged');
    }
  }
  final merged = <FollowUpRow>[
    for (var i = 0; i < revisedRows.length; i++)
      revisedRows[i].copyWith(
        enabled: revisedRows[i].enabled && freshRows[i].selectable,
      ),
  ];
  final warnings = [
    for (final warning in fresh.warnings)
      if (warning.key != 'warn.rfi.followUp.noRowsSelected') warning,
  ];
  if (merged.every((row) => !row.enabled)) {
    warnings.add(
      const Warning(key: 'warn.rfi.followUp.noRowsSelected', subject: ''),
    );
  }
  final classification = <String, Object?>{
    'layout': authority.layout.toJson(),
    'layouts': [for (final item in authority.layouts) item.toJson()],
    'rows': [for (final row in merged) row.toJson()],
    'warnings': [for (final warning in warnings) warning.toJson()],
    'editor': editorMetadata(merged, editorColumns()),
  };
  final revised = planFromClassification(
    rfiLogWorkbook: authority.rfiLogWorkbook,
    followUpWorkbook: authority.followUpWorkbook,
    backupWorkbook: authority.backupWorkbook,
    rfiLogFingerprint: freshAuthority.rfiLogFingerprint,
    followUpFingerprint: freshAuthority.followUpFingerprint,
    classification: classification,
  );
  final result = Plan(
    planId: plan.planId,
    opId: revised.opId,
    steps: revised.steps,
    warnings: revised.warnings,
    meta: revised.meta,
  );
  validateOwnedPlanBudget(result);
  return result;
}

// ---------------------------------------------------------------------------
// Classification entry points
// ---------------------------------------------------------------------------

/// Port of `classify_workbooks`: full-schema path when the RFI Log exposes the
/// CIV/MEC header schema and the target has an approved sheet; else legacy.
JsonObject classifyWorkbooks(
  OrderedWorkbookDiscovery sourceDiscovery,
  WorkbookData sourceWorkbook,
  OrderedWorkbookDiscovery targetDiscovery,
  WorkbookData targetWorkbook, {
  required bool targetHasSignature,
}) {
  if (supportsFullSyncSchema(sourceWorkbook) &&
      targetWorkbook.sheets.keys.any((name) => approvedSheet(rustTrim(name)))) {
    return classifyWorkbooksFull(
      sourceDiscovery,
      sourceWorkbook,
      targetDiscovery,
      targetWorkbook,
      targetHasSignature: targetHasSignature,
    );
  }
  return classifyWorkbooksLegacy(
    sourceDiscovery,
    sourceWorkbook,
    targetDiscovery,
    targetWorkbook,
    targetHasSignature: targetHasSignature,
  );
}

/// Port of `supports_full_sync_schema`.
bool supportsFullSyncSchema(WorkbookData workbook) {
  return const ['CIV', 'MEC'].every((name) {
    final sheet = findSheetFlexible(workbook, name);
    return sheet != null &&
        findHeaderColumn(sheet, const ['RFI NO.', 'RFI NO', 'RFI #']) != null &&
        findHeaderColumn(sheet, const ['ITP NO:', 'ITP NO.', 'ITP NO']) != null &&
        findHeaderColumn(sheet, const ['ITEM NO.', 'ITEM NO', 'ITEM #']) != null &&
        findHeaderColumn(sheet, const ['INSPECTION STATUS', 'STATUS']) != null &&
        findHeaderColumn(sheet, const ['REMARKS', 'REMARK']) != null;
  });
}

/// Port of `classify_workbooks_full`.
JsonObject classifyWorkbooksFull(
  OrderedWorkbookDiscovery sourceDiscovery,
  WorkbookData sourceWorkbook,
  OrderedWorkbookDiscovery targetDiscovery,
  WorkbookData targetWorkbook, {
  required bool targetHasSignature,
}) {
  final records = scanSourceRecords(sourceDiscovery, sourceWorkbook);
  final mergeBudget = FollowUpMergeBudget(_maxFollowUpMergeComparisons);
  final layouts = discoverApprovedLayouts(
    targetDiscovery,
    targetWorkbook,
    targetHasSignature,
    mergeBudget,
  );
  final slots = SplayTreeMap<SlotKey, SlotWork>(compareSlotKey);
  for (final layout in layouts) {
    final sheet = targetWorkbook.sheet(layout.worksheet.name);
    if (sheet == null) throw _workbookInvalidError();
    final last = min(sheet.maxRow, layout.headerRow + _maxFollowUpDataRows);
    for (var row = layout.headerRow + 1; row <= last; row++) {
      final slot = safeSlot(layout, sheet, row, mergeBudget);
      if (slot != null) {
        slots[(
              layout.worksheet.name,
              row,
              layout.rfiColumn,
              layout.statusColumn,
            )] =
            slot;
      }
    }
  }

  final manualRows = <FollowUpRow>[];
  for (final slot in slots.values) {
    final codes = slot.codes;
    final removed = [
      for (final code in codes)
        if (!isPlaceholderRfi(code) &&
            records[code] != null &&
            (records[code]!.excludedCivPcs ||
                isConfirmedFollowUpExclusion(records[code]!) ||
                records[code]!.state == LogState.remove))
          code,
    ];
    if (removed.isNotEmpty) {
      slot.codes = [
        for (final code in codes)
          if (!removed.contains(code)) code,
      ];
      slot.changedRfis.addAll(removed);
    }
    if (!isSignedLayout(slot.layout)) {
      final real = slot.codes.where((code) => !isPlaceholderRfi(code)).toList();
      final allMatch = real.every((code) {
        final record = records[code];
        return record != null && recordMatchesLifecycleSlot(slot, record, targetWorkbook);
      });
      final anyState = real.any((code) => records[code]?.state != null);
      if (allMatch && anyState) {
        final next = mainStatusForCodes(slot.codes, records);
        if (next != null && next != slot.currentStatus) {
          slot.changedRfis.addAll(slot.codes);
        }
      }
    }
  }

  final potentialSlotsBySheetCol =
      SplayTreeMap<(String, int), List<SlotKey>>(_compareSheetCol);
  final existingSlotKeysByCode = SplayTreeMap<String, List<SlotKey>>(
    compareCodePoints,
  );
  for (final entry in slots.entries) {
    final key = entry.key;
    final slot = entry.value;
    if (!isSignedLayout(slot.layout)) {
      for (final code in slot.codes) {
        existingSlotKeysByCode.putIfAbsent(code, () => []).add(key);
      }
      potentialSlotsBySheetCol.putIfAbsent((key.$1, key.$3), () => []).add(key);
    }
  }

  var unmappedCount = 0;
  for (final record in records.values.where(
    (record) =>
        !record.excludedCivPcs &&
        !isConfirmedFollowUpExclusion(record) &&
        record.state != LogState.remove,
  )) {
    final route = routeClause(record);
    if (route == null) {
      if (slots.values.any((slot) => slot.codes.contains(record.canonicalRfi))) {
        continue;
      }
      unmappedCount += 1;
      final prefix = record.shortRfi.split('-').first;
      final layout = layouts.where(
        (layout) => eqIgnoreAsciiCase(rustTrim(layout.worksheet.name), prefix),
      );
      if (layout.isNotEmpty) {
        final l = layout.first;
        manualRows.add(
          FollowUpRow(
            rowId: 'rfi-follow-up-unmapped:${record.canonicalRfi}',
            sheet: l.worksheet.name,
            row: 0,
            rfiNo: record.shortRfi,
            sourceStatuses: [record.inspectionStatusForUi()],
            currentStatus: '',
            proposedStatus: null,
            proposedRfiNo: null,
            disposition: FollowUpDisposition.manualReview,
            reasonKey: record.state == null
                ? 'warn.rfi.followUp.reason.sourceStatusUnknown'
                : 'warn.rfi.followUp.reason.mappingUnsupported',
            selectable: false,
            enabled: false,
            target: FollowUpReviewTarget(
              worksheet: l.worksheet,
              row: 0,
              rfiColumn: l.rfiColumn,
              statusColumn: l.statusColumn,
            ),
            sourceEvidence: record.evidence,
            mappingDetails: mappingDetails(record, null, targetWorkbook, const []),
          ),
        );
      }
      continue;
    }
    var (targetSheet, targetClause) = route;
    if (compositePcsRoute(record)) {
      targetSheet = 'CIV';
      targetClause = switch (targetClause) {
        '1.8' => '1.8',
        '2.4' => '2.4',
        _ => '2.5',
      };
    }
    final matchingLayouts = layouts.where(
      (l) =>
          eqIgnoreAsciiCase(rustTrim(l.worksheet.name), targetSheet) &&
          (l.clause == targetClause ||
              (l.clause == null &&
                  l.rfiColumn == defaultRouteCol(targetSheet, targetClause))),
    );
    if (matchingLayouts.isEmpty) {
      // FINAL routes are reconciled separately against Signed By rows.
      if (signedRouteClause(record) != null) continue;
      unmappedCount += 1;
      final named = layouts.where(
        (l) => eqIgnoreAsciiCase(rustTrim(l.worksheet.name), targetSheet),
      );
      if (named.isNotEmpty) {
        final l = named.first;
        manualRows.add(
          FollowUpRow(
            rowId: 'rfi-follow-up-unmapped:${record.canonicalRfi}',
            sheet: targetSheet,
            row: 0,
            rfiNo: record.shortRfi,
            sourceStatuses: [record.inspectionStatusForUi()],
            currentStatus: '',
            proposedStatus: null,
            proposedRfiNo: null,
            disposition: FollowUpDisposition.manualReview,
            reasonKey: record.state == null
                ? 'warn.rfi.followUp.reason.sourceStatusUnknown'
                : 'warn.rfi.followUp.reason.mappingMissing',
            selectable: false,
            enabled: false,
            target: FollowUpReviewTarget(
              worksheet: l.worksheet,
              row: 0,
              rfiColumn: defaultRouteCol(targetSheet, targetClause),
              statusColumn: defaultRouteCol(targetSheet, targetClause) + 1,
            ),
            sourceEvidence: record.evidence,
            mappingDetails: mappingDetails(
              record,
              (targetSheet, targetClause),
              targetWorkbook,
              const [],
            ),
          ),
        );
      }
      continue;
    }
    final layout = matchingLayouts.first;
    final sheetName = layout.worksheet.name;
    final rfiColumn = layout.rfiColumn;
    final potential =
        potentialSlotsBySheetCol[(sheetName, rfiColumn)] ?? const <SlotKey>[];
    final targetSheetData = targetWorkbook.sheet(sheetName);
    if (targetSheetData == null) throw _workbookInvalidError();
    final scopeCandidates = <SlotKey>[];
    final candidates = <SlotKey>[];
    for (final key in potential) {
      if (rowMatches(sheetName, targetSheetData, key.$2, record, targetWorkbook)) {
        scopeCandidates.add(key);
        final slot = slots[key];
        if (slot != null && slot.unsafeReason == null) candidates.add(key);
      }
    }
    final candidateCount = candidates.length;
    final civCoverage = sheetName == 'CIV'
        ? civTagCoverage(targetWorkbook, candidates, record)
        : null;
    final mecCoverage = sheetName == 'MEC'
        ? mecContextCoverage(targetWorkbook, sheetName, candidates, record)
        : null;
    final existingSlots = [
      ...?existingSlotKeysByCode[record.canonicalRfi],
    ];
    if (scopeCandidates.isEmpty) continue;
    if (candidateCount == 1 &&
        existingSlots.isNotEmpty &&
        mecCoverage != false &&
        civCoverage != false) {
      continue;
    }
    final exactMultiArea = candidateCount > 1 &&
        ((sheetName == 'CIV' &&
                (distinctCivAreaRows(targetWorkbook, candidates, record) ||
                    distinctCivZoneAreaRows(targetWorkbook, candidates, record) ||
                    civCoverage == true)) ||
            (sheetName == 'MEC' && mecCoverage != null));
    if ((candidateCount == 0 || (candidateCount > 1 && !exactMultiArea)) &&
        (candidateCount == 0 || existingSlots.isEmpty)) {
      unmappedCount += 1;
      final matches = layouts.where(
        (l) => l.worksheet.name == sheetName && l.rfiColumn == rfiColumn,
      );
      if (matches.isEmpty) {
        unmappedCount -= 1;
        continue;
      }
      final l = matches.first;
      String reason;
      if (record.state == null) {
        reason = 'warn.rfi.followUp.reason.sourceStatusUnknown';
      } else if (candidateCount == 0) {
        String? unsafe;
        for (final key in scopeCandidates) {
          final reasonKey = slots[key]?.unsafeReason;
          if (reasonKey != null) {
            unsafe = reasonKey;
            break;
          }
        }
        reason = unsafe != null
            ? 'warn.rfi.followUp.reason.$unsafe'
            : 'warn.rfi.followUp.reason.mappingMissing';
      } else {
        reason = 'warn.rfi.followUp.reason.mappingAmbiguous';
      }
      manualRows.add(
        FollowUpRow(
          rowId: 'rfi-follow-up-unmapped:${record.canonicalRfi}',
          sheet: sheetName,
          row: 0,
          rfiNo: record.shortRfi,
          sourceStatuses: [record.inspectionStatusForUi()],
          currentStatus: '',
          proposedStatus: null,
          proposedRfiNo: null,
          disposition: FollowUpDisposition.manualReview,
          reasonKey: reason,
          selectable: false,
          enabled: false,
          target: FollowUpReviewTarget(
            worksheet: l.worksheet,
            row: 0,
            rfiColumn: l.rfiColumn,
            statusColumn: l.statusColumn,
          ),
          sourceEvidence: record.evidence,
          mappingDetails: mappingDetails(
            record,
            (sheetName, targetClause),
            targetWorkbook,
            scopeCandidates,
          ),
        ),
      );
      continue;
    }
    final alreadyMappedElsewhere = existingSlots.any(
      (key) => !candidates.contains(key),
    );
    if (alreadyMappedElsewhere) {
      unmappedCount += 1;
      final l = _slotLayout(slots, candidates);
      manualRows.add(
        FollowUpRow(
          rowId: 'rfi-follow-up-unmapped:${record.canonicalRfi}',
          sheet: sheetName,
          row: 0,
          rfiNo: record.shortRfi,
          sourceStatuses: [record.inspectionStatusForUi()],
          currentStatus: '',
          proposedStatus: null,
          proposedRfiNo: null,
          disposition: FollowUpDisposition.manualReview,
          reasonKey: 'warn.rfi.followUp.reason.mappingAmbiguous',
          selectable: false,
          enabled: false,
          target: FollowUpReviewTarget(
            worksheet: l.worksheet,
            row: 0,
            rfiColumn: l.rfiColumn,
            statusColumn: l.statusColumn,
          ),
          sourceEvidence: record.evidence,
          mappingDetails: mappingDetails(
            record,
            (sheetName, targetClause),
            targetWorkbook,
            scopeCandidates,
          ),
        ),
      );
      continue;
    }
    if (mecCoverage == false || civCoverage == false) {
      unmappedCount += 1;
      final l = _slotLayout(slots, candidates);
      manualRows.add(
        FollowUpRow(
          rowId: 'rfi-follow-up-unmapped:${record.canonicalRfi}:missing-context',
          sheet: sheetName,
          row: 0,
          rfiNo: record.shortRfi,
          sourceStatuses: [record.inspectionStatusForUi()],
          currentStatus: '',
          proposedStatus: null,
          proposedRfiNo: null,
          disposition: FollowUpDisposition.manualReview,
          reasonKey: record.state == null
              ? 'warn.rfi.followUp.reason.sourceStatusUnknown'
              : 'warn.rfi.followUp.reason.partialContext',
          selectable: false,
          enabled: false,
          target: FollowUpReviewTarget(
            worksheet: l.worksheet,
            row: 0,
            rfiColumn: l.rfiColumn,
            statusColumn: l.statusColumn,
          ),
          sourceEvidence: record.evidence,
          mappingDetails: mappingDetails(
            record,
            (sheetName, targetClause),
            targetWorkbook,
            scopeCandidates,
          ),
        ),
      );
    }

    for (final key in candidates) {
      final slot = slots[key];
      if (slot == null) throw _workbookInvalidError();
      if (!slot.codes.contains(record.canonicalRfi)) {
        if (!slotCompatibleWith(slot, record, records)) continue;
        slot.codes.add(record.canonicalRfi);
        slot.changedRfis.add(record.canonicalRfi);
      }
    }
  }

  final rows = <FollowUpRow>[];
  for (final slot in [
    for (final s in slots.values)
      if (!isSignedLayout(s.layout)) s,
  ]) {
    final sourceIssue = _firstSourceIssue(slot, records);
    if (slot.unsafeReason == null && sourceIssue != null) {
      FollowUpRecord? found;
      for (final code in slot.codes) {
        if (isPlaceholderRfi(code)) continue;
        final r = records[code];
        if (r != null) {
          found = FollowUpRecord(r);
          break;
        }
      }
      if (found != null) {
        final record = found.record;
        manualRows.add(
          FollowUpRow(
            rowId:
                'rfi-follow-up-source-unsafe:${slot.layout.worksheet.sheetId}:${slot.row}:${slot.layout.rfiColumn}',
            sheet: slot.layout.worksheet.name,
            row: slot.row,
            rfiNo: record.shortRfi,
            sourceStatuses: [record.inspectionStatusForUi()],
            currentStatus: slot.currentStatus,
            proposedStatus: null,
            proposedRfiNo: null,
            disposition: FollowUpDisposition.manualReview,
            reasonKey: 'warn.rfi.followUp.reason.$sourceIssue',
            selectable: false,
            enabled: false,
            target: FollowUpReviewTarget(
              worksheet: slot.layout.worksheet,
              row: slot.row,
              rfiColumn: slot.layout.rfiColumn,
              statusColumn: slot.layout.statusColumn,
              rfiCell: slot.rfiCell,
              statusCell: slot.statusCell,
            ),
            sourceEvidence: record.evidence,
            mappingDetails: null,
          ),
        );
        continue;
      }
    }
    if (slot.unsafeReason != null && slot.changedRfis.isNotEmpty) {
      final row = unsafeSlotReview(slot, records);
      if (row != null) {
        unmappedCount += 1;
        manualRows.add(row);
      }
      continue;
    }
    final row = rowFromSlot(slot, records, signed: false);
    if (row != null) rows.add(row);
  }

  final routedSignedTargets = <String, Set<(int, int, int, int)>>{};
  for (final record in records.values.where(
    (record) =>
        !record.excludedCivPcs &&
        !isConfirmedFollowUpExclusion(record) &&
        record.state != LogState.remove &&
        signedRouteClause(record) != null,
  )) {
    final signed = signedRouteClause(record)!;
    final targetSheet = signed.$1;
    final targetClause = signed.$2;
    final matchSheet = signed.$3;
    final matchingLayouts = layouts.where(
      (l) =>
          eqIgnoreAsciiCase(rustTrim(l.worksheet.name), targetSheet) &&
          (l.clause == targetClause ||
              (rustTrim(targetSheet).toUpperCase().contains('PCU') &&
                  l.statusColumn == 18) ||
              (l.clause == null &&
                  l.rfiColumn == defaultSignedCol(targetSheet, targetClause).$1)),
    );
    final finalReviewId = 'rfi-follow-up-unmapped:${record.canonicalRfi}';
    if (record.state == null) {
      if (!manualRows.any((row) => row.rowId == finalReviewId)) {
        final layout = matchingLayouts.isNotEmpty
            ? matchingLayouts.first
            : _firstWhereOrNull(
                    layouts,
                    (l) => eqIgnoreAsciiCase(rustTrim(l.worksheet.name), targetSheet),
                  ) ??
                  (layouts.isNotEmpty ? layouts.first : null);
        if (layout != null) {
          unmappedCount += 1;
          final cols = defaultSignedCol(targetSheet, targetClause);
          final review = missingFinalReview(
            record,
            layout,
            targetSheet,
            targetClause,
            cols,
            const [],
            targetWorkbook,
          );
          manualRows.add(_withReason(review, 'warn.rfi.followUp.reason.sourceStatusUnknown'));
        }
      }
      continue;
    }
    if (matchingLayouts.isEmpty) {
      if (!manualRows.any((row) => row.rowId == finalReviewId)) {
        final layout = _firstWhereOrNull(
          layouts,
          (l) => eqIgnoreAsciiCase(rustTrim(l.worksheet.name), targetSheet),
        ) ??
            (layouts.isNotEmpty ? layouts.first : null);
        if (layout != null) {
          unmappedCount += 1;
          final cols = defaultSignedCol(targetSheet, targetClause);
          manualRows.add(
            missingFinalReview(
              record,
              layout,
              targetSheet,
              targetClause,
              cols,
              const [],
              targetWorkbook,
            ),
          );
        }
      }
      continue;
    }
    final layout = matchingLayouts.first;
    final sheetName = layout.worksheet.name;
    final rfiColumn = layout.rfiColumn;
    final statusColumn = layout.statusColumn;
    final matchingSheet = targetWorkbook.sheet(sheetName) ??
        findSheetFlexible(targetWorkbook, sheetName);
    if (matchingSheet == null) continue;
    final candidateKeys = <SlotKey>[
      for (final key in slots.keys)
        if (key.$1 == sheetName &&
            key.$3 == rfiColumn &&
            key.$4 == statusColumn &&
            rowMatches(matchSheet, matchingSheet, key.$2, record, targetWorkbook))
          key,
    ];
    final candidateCount = candidateKeys.length;
    final hasUnrelatedPlacement = slots.entries.any(
      (entry) =>
          entry.key.$1 == sheetName &&
          entry.key.$3 == rfiColumn &&
          entry.key.$4 == statusColumn &&
          entry.value.codes.contains(record.canonicalRfi) &&
          !candidateKeys.contains(entry.key),
    );
    if (candidateCount == 0) continue;
    final coverage = signedDestinationCoverage(
      sheetName,
      matchingSheet,
      candidateKeys,
      record,
    );
    final canProcess = !hasUnrelatedPlacement && coverage.valid;
    if (canProcess) {
      for (final candidateKey in candidateKeys) {
        final slot = slots[candidateKey];
        if (slot == null) throw _workbookInvalidError();
        if (sheetName == 'PCU' && slot.codes.isEmpty) continue;
        if (slot.unsafeReason == 'targetFormula' &&
            slot.codes.contains(record.canonicalRfi) &&
            slot.currentStatus ==
                (record.worleySigned ? 'Signed' : 'Not Signed')) {
          routedSignedTargets.putIfAbsent(record.canonicalRfi, () => <(int, int, int, int)>{}).add((
            slot.layout.worksheet.sheetId,
            slot.row,
            slot.layout.rfiColumn,
            slot.layout.statusColumn,
          ));
          continue;
        }
        final reason = slot.unsafeReason;
        if (reason != null) {
          unmappedCount += 1;
          manualRows.add(
            FollowUpRow(
              rowId:
                  'rfi-follow-up-signed-unsafe:${record.canonicalRfi}:${slot.row}:${slot.layout.rfiColumn}',
              sheet: sheetName,
              row: slot.row,
              rfiNo: record.shortRfi,
              sourceStatuses: [record.worleySigned ? 'Signed' : 'Not Signed'],
              currentStatus: slot.currentStatus,
              proposedStatus: null,
              proposedRfiNo: null,
              disposition: FollowUpDisposition.manualReview,
              reasonKey: 'warn.rfi.followUp.reason.$reason',
              selectable: false,
              enabled: false,
              target: FollowUpReviewTarget(
                worksheet: slot.layout.worksheet,
                row: slot.row,
                rfiColumn: slot.layout.rfiColumn,
                statusColumn: slot.layout.statusColumn,
                rfiCell: slot.rfiCell,
                statusCell: slot.statusCell,
              ),
              sourceEvidence: record.evidence,
              mappingDetails: null,
            ),
          );
          continue;
        }
        if (!slot.codes.contains(record.canonicalRfi)) {
          if (!slotCompatibleWith(slot, record, records)) continue;
          slot.codes.add(record.canonicalRfi);
          slot.changedRfis.add(record.canonicalRfi);
        }
        routedSignedTargets.putIfAbsent(record.canonicalRfi, () => <(int, int, int, int)>{}).add((
          slot.layout.worksheet.sheetId,
          slot.row,
          slot.layout.rfiColumn,
          slot.layout.statusColumn,
        ));
      }
      if (coverage.missingContexts &&
          !manualRows.any((row) => row.rowId == finalReviewId)) {
        unmappedCount += 1;
        final review = missingFinalReview(
          record,
          layout,
          targetSheet,
          targetClause,
          (rfiColumn, statusColumn),
          candidateKeys,
          targetWorkbook,
        );
        manualRows.add(_withReason(review, 'warn.rfi.followUp.reason.partialContext'));
      }
      continue;
    }
    unmappedCount += 1;
    final exact = _firstWhereOrNull(
      layouts,
      (l) =>
          l.worksheet.name == sheetName &&
          l.rfiColumn == rfiColumn &&
          l.statusColumn == statusColumn,
    );
    if (exact == null) {
      unmappedCount -= 1;
      continue;
    }
    manualRows.add(
      FollowUpRow(
        rowId: 'rfi-follow-up-signed-unmapped:${record.canonicalRfi}',
        sheet: sheetName,
        row: 0,
        rfiNo: record.shortRfi,
        sourceStatuses: [record.worleySigned ? 'Signed' : 'Not Signed'],
        currentStatus: '',
        proposedStatus: null,
        proposedRfiNo: null,
        disposition: FollowUpDisposition.manualReview,
        reasonKey: candidateCount == 0
            ? 'warn.rfi.followUp.reason.mappingMissing'
            : 'warn.rfi.followUp.reason.mappingAmbiguous',
        selectable: false,
        enabled: false,
        target: FollowUpReviewTarget(
          worksheet: exact.worksheet,
          row: 0,
          rfiColumn: exact.rfiColumn,
          statusColumn: exact.statusColumn,
        ),
        sourceEvidence: record.evidence,
        mappingDetails: mappingDetails(
          record,
          (targetSheet, targetClause),
          targetWorkbook,
          candidateKeys,
        ),
      ),
    );
  }

  for (final slot in slots.values.where((s) => isSignedLayout(s.layout))) {
    final original = [...slot.codes];
    slot.codes = [
      for (final code in slot.codes)
        if (isPlaceholderRfi(code) ||
            records[code]?.state != LogState.remove)
          code,
    ];
    if (!_listEq(slot.codes, original)) {
      for (final code in original) {
        if (!slot.codes.contains(code)) slot.changedRfis.add(code);
      }
    }
    for (final code in slot.codes) {
      final record = records[code];
      if (record != null &&
          (routedSignedTargets[code]?.contains((
                slot.layout.worksheet.sheetId,
                slot.row,
                slot.layout.rfiColumn,
                slot.layout.statusColumn,
              )) ??
              false)) {
        final proposed = record.worleySigned ? 'Signed' : 'Not Signed';
        if (proposed != slot.currentStatus) slot.changedRfis.add(code);
      }
    }
  }
  for (final slot in [
    for (final s in slots.values)
      if (isSignedLayout(s.layout)) s,
  ]) {
    final hasUnmatchedSource = slot.codes.any((code) {
      if (isPlaceholderRfi(code)) return false;
      final record = records[code];
      if (record == null) return false;
      return record.state == null ||
          !(routedSignedTargets[code]?.contains((
                slot.layout.worksheet.sheetId,
                slot.row,
                slot.layout.rfiColumn,
                slot.layout.statusColumn,
              )) ??
              false);
    });
    if (hasUnmatchedSource) continue;
    if (slot.unsafeReason != null && slot.changedRfis.isNotEmpty) {
      final row = unsafeSlotReview(slot, records);
      if (row != null) {
        unmappedCount += 1;
        manualRows.add(row);
      }
      continue;
    }
    final row = rowFromSlot(slot, records, signed: true);
    if (row != null) rows.add(row);
  }
  rows.addAll(manualRows);

  if (rows.length > _maxPreviewRows) throw limitExceededError();
  final sortedRows = stableSort(rows, (left, right) {
    final bySheet = compareCodePoints(left.sheet, right.sheet);
    if (bySheet != 0) return bySheet;
    final byRow = left.row.compareTo(right.row);
    if (byRow != 0) return byRow;
    return left.target.rfiColumn.compareTo(right.target.rfiColumn);
  });

  final warnings = <Warning>[];
  if (unmappedCount > 0) {
    warnings.add(
      Warning(
        key: 'warn.rfi.followUp.unmappedRecords',
        subject: unmappedCount.toString(),
      ),
    );
  }
  if (sortedRows.isEmpty) {
    warnings.add(const Warning(key: 'warn.rfi.followUp.noRowsSelected', subject: ''));
  }
  final primary = layouts.first;
  final projection = <String, Object?>{
    'layout': primary.toJson(),
    'layouts': [for (final item in layouts) item.toJson()],
    'rows': [for (final row in sortedRows) row.toJson()],
    'warnings': [for (final warning in warnings) warning.toJson()],
    'editor': editorMetadata(sortedRows, editorColumns()),
  };
  validateProjectionBudget(projection);
  return projection;
}

/// Port of `classify_workbooks_legacy`.
JsonObject classifyWorkbooksLegacy(
  OrderedWorkbookDiscovery sourceDiscovery,
  WorkbookData sourceWorkbook,
  OrderedWorkbookDiscovery targetDiscovery,
  WorkbookData targetWorkbook, {
  required bool targetHasSignature,
}) {
  final source = scanSource(sourceDiscovery, sourceWorkbook);
  final mergeBudget = FollowUpMergeBudget(_maxFollowUpMergeComparisons);
  final analyses = <String, SourceAnalysis>{};
  for (final entry in source.entries) {
    analyses[entry.key] = analyzeSource(entry.value, sourceWorkbook, mergeBudget);
  }
  const emptyAnalysis = SourceAnalysis(statuses: [], proposedStatus: null, reason: null);
  final layouts = discoverLayouts(
    targetDiscovery,
    targetWorkbook,
    targetHasSignature,
    mergeBudget,
  );
  final allPending = <(FollowUpLayout, WorksheetData, PendingTargetRow)>[];
  for (final layout in layouts) {
    final worksheet = targetWorkbook.sheet(layout.worksheet.name);
    if (worksheet == null) throw _workbookInvalidError();
    for (final row in scanTargetRows(worksheet, layout)) {
      allPending.add((layout, worksheet, row));
    }
  }
  final duplicateCounts = <String, int>{};
  for (final (_, _, pending) in allPending) {
    final rfi = pending.canonicalRfi;
    if (rfi != null) duplicateCounts[rfi] = (duplicateCounts[rfi] ?? 0) + 1;
  }
  final borrowedRows = <BorrowedRow>[];
  for (final (layout, worksheet, pending) in allPending) {
    final rfi = pending.canonicalRfi;
    final evidence = rfi == null
        ? const <FollowUpSourceEvidence>[]
        : (source[rfi] ?? const <FollowUpSourceEvidence>[]);
    final analysis = rfi == null
        ? emptyAnalysis
        : (analyses[rfi] ?? emptyAnalysis);
    borrowedRows.add(
      classifyBorrowedRow(
        pending,
        layout,
        worksheet,
        evidence,
        analysis,
        duplicateCounts,
        mergeBudget,
      ),
    );
  }
  final manualCount = borrowedRows
      .where((row) => row.disposition == FollowUpDisposition.manualReview)
      .length;
  final enabledCount = borrowedRows.where((row) => row.enabled).length;
  final warnings = <Warning>[];
  if (manualCount > 0) {
    warnings.add(
      Warning(key: 'warn.rfi.followUp.manualRows', subject: manualCount.toString()),
    );
  }
  if (enabledCount == 0) {
    warnings.add(const Warning(key: 'warn.rfi.followUp.noRowsSelected', subject: ''));
  }
  final columns = editorColumnsLegacy();
  final primary = layouts.first;
  final rows = [for (final row in borrowedRows) row.materialize()];
  final approved = [
    for (final row in borrowedRows)
      if (row.enabled) row.toBudgetJson(),
  ];
  validateProjectionBudget({
    'layout': primary.toJson(),
    'layouts': [for (final item in layouts) item.toJson()],
    'rows': [for (final row in borrowedRows) row.toBudgetJson()],
    'warnings': [for (final warning in warnings) warning.toJson()],
    'editor': {
      ...editorMetadata(const [], columns),
      'rows': [for (final row in borrowedRows) row.toBudgetJson()],
    },
    'approvedRows': approved,
  });
  return {
    'layout': primary.toJson(),
    'layouts': [for (final item in layouts) item.toJson()],
    'rows': [for (final row in rows) row.toJson()],
    'warnings': [for (final warning in warnings) warning.toJson()],
    'editor': editorMetadata(rows, columns),
  };
}

/// Port of `preview_from_open_files`'s legacy counterpart is covered by
/// [previewFromOpenFiles]; this is the public legacy entry point used by tests.
JsonObject classifyLegacyPublic(
  OrderedWorkbookDiscovery sourceDiscovery,
  WorkbookData sourceWorkbook,
  OrderedWorkbookDiscovery targetDiscovery,
  WorkbookData targetWorkbook, {
  required bool targetHasSignature,
}) => classifyWorkbooksLegacy(
  sourceDiscovery,
  sourceWorkbook,
  targetDiscovery,
  targetWorkbook,
  targetHasSignature: targetHasSignature,
);

// ---------------------------------------------------------------------------
// Public RFI and status helpers (port of the `pub fn` surface)
// ---------------------------------------------------------------------------

/// Port of `canonical_rfi`.
String? canonicalRfi(String value) {
  final normalized = rustUpper(normalizeStatus(value));
  final match = _fullRfiRe.firstMatch(normalized);
  if (match == null) return null;
  final number = normalizeCandidateNumber(match.group(2)!);
  if (number == null) return null;
  return '$_fullRfiPrefix${match.group(1)}-$number';
}

/// Port of `canonical_follow_up_rfi`: returns (canonical, shortCode).
(String, String)? canonicalFollowUpRfi(String value) {
  final normalized = rustUpper(normalizeStatus(value));
  final short = normalized.startsWith(_fullRfiPrefix)
      ? normalized.substring(_fullRfiPrefix.length)
      : normalized;
  final match = _shortRfiRe.firstMatch(short);
  if (match == null) return null;
  final number = normalizeCandidateNumber(match.group(2)!);
  if (number == null) return null;
  final canonicalShort = '${match.group(1)}-$number';
  return ('$_fullRfiPrefix$canonicalShort', canonicalShort);
}

/// Port of `is_placeholder_rfi`.
bool isPlaceholderRfi(String code) {
  final index = code.lastIndexOf('-');
  final digits = index >= 0 ? code.substring(index + 1) : code;
  final number = parseU32(digits);
  return number != null && number >= 2000;
}

/// Port of `mapped_follow_up_status`.
String mappedFollowUpStatus(String remark) {
  for (final line in _rustLines(remark)) {
    if (normalizeStatus(line) == _worleySignedMarker) return 'Signed';
  }
  return 'Not Signed';
}

/// Port of `aggregate_status`.
String aggregateStatus(Iterable<LogState> states) {
  final filtered = [
    for (final state in states)
      if (state != LogState.remove) state,
  ];
  if (filtered.isEmpty) return '';
  if (filtered.every((state) => state == LogState.pending)) return 'Pending';
  if (filtered.every((state) => state == LogState.completed)) return 'Completed';
  return 'In Progress';
}

/// Port of `log_state`.
LogState? logState(String value) {
  return switch (normalizeHeader(value)) {
    '' => LogState.pending,
    'ONGOING' || 'IN PROGRESS' => LogState.inProgress,
    'COMPLETED' => LogState.completed,
    'CANCELLED' || 'CANCELED' || 'REJECTED' => LogState.remove,
    _ => null,
  };
}

/// Port of `parse_rfi_group`. Returns canonical codes, or null when invalid.
List<String>? parseRfiGroup(String value) {
  var normalized = rustUpper(normalizeStatus(value)).replaceAll(',', '/');
  if (normalized.startsWith(_fullRfiPrefix)) {
    normalized = normalized.substring(_fullRfiPrefix.length);
  } else if (normalized.startsWith('RFI-')) {
    normalized = normalized.substring(4);
  }
  String? discipline;
  final numbers = <int>[];
  for (final rawPart in normalized.split('/')) {
    final part = rustTrim(rawPart);
    if (part.isEmpty) return null;
    final captures = _partRe.firstMatch(part);
    if (captures == null) return null;
    if (captures.group(1) != null) {
      final found = captures.group(1)!;
      if (discipline != null && discipline != found) return null;
      discipline ??= found;
    }
    final startText = normalizeCandidateNumber(captures.group(2)!);
    if (startText == null) return null;
    final start = int.parse(startText);
    var end = start;
    final endRaw = captures.group(3);
    if (endRaw != null) {
      final endText = normalizeCandidateNumber(endRaw);
      if (endText != null) end = int.parse(endText);
    }
    if (end < start || end - start > 500) return null;
    for (var n = start; n <= end; n++) {
      numbers.add(n);
    }
  }
  if (discipline == null) return null;
  final seen = <int>{};
  final result = [
    for (final number in numbers)
      if (seen.add(number))
        '$_fullRfiPrefix$discipline-${number.toString().padLeft(4, '0')}',
  ];
  return result.isEmpty ? null : result;
}

/// Port of `format_rfi_group`.
String? formatRfiGroup(List<String> codes) {
  String? discipline;
  final numbers = <String>[];
  for (final code in codes) {
    final canonical = canonicalFollowUpRfi(code);
    if (canonical == null) return null;
    final short = canonical.$2;
    final dash = short.lastIndexOf('-');
    if (dash < 0) return null;
    final found = short.substring(0, dash);
    final number = short.substring(dash + 1);
    if (discipline != null && discipline != found) return null;
    discipline ??= found;
    numbers.add(number);
  }
  if (discipline == null) return null;
  return '$discipline-${numbers.join('/')}';
}

// ---------------------------------------------------------------------------
// Source scan
// ---------------------------------------------------------------------------

/// Port of `LogState`.
enum LogState { pending, inProgress, completed, remove }

/// Port of `SourceRouteContext`.
class SourceRouteContext {
  const SourceRouteContext({
    required this.tag,
    required this.area,
    required this.location,
  });

  final String tag;
  final String area;
  final String location;

  @override
  bool operator ==(Object other) =>
      other is SourceRouteContext &&
      other.tag == tag &&
      other.area == area &&
      other.location == location;

  @override
  int get hashCode => Object.hash(tag, area, location);
}

/// Port of `SourceRecord`.
class SourceRecord {
  SourceRecord({
    required this.canonicalRfi,
    required this.shortRfi,
    required this.itp,
    required this.item,
    required this.state,
    required this.excludedCivPcs,
    required this.worleySigned,
    required this.contexts,
    required this.evidence,
  });

  final String canonicalRfi;
  final String shortRfi;
  final String itp;
  final String item;
  LogState? state;
  bool excludedCivPcs;
  bool worleySigned;
  final List<SourceRouteContext> contexts;
  final List<FollowUpSourceEvidence> evidence;

  String inspectionStatusForUi() => switch (state) {
    LogState.pending => 'Pending',
    LogState.inProgress => 'In Progress',
    LogState.completed => 'Completed',
    LogState.remove => 'Cancelled / Rejected',
    null => 'Manual review',
  };
}

/// Port of `scan_source_records`: the full CIV/MEC source scan.
SplayTreeMap<String, SourceRecord> scanSourceRecords(
  OrderedWorkbookDiscovery discovery,
  WorkbookData workbook,
) {
  if (discovery.safety.hasWorkbookProtection) throw _protectedError();
  final records = SplayTreeMap<String, SourceRecord>(compareCodePoints);
  for (final discipline in const ['CIV', 'MEC']) {
    final descriptor = _firstWhereOrNull(
      discovery.worksheets,
      (sheet) => eqIgnoreAsciiCase(rustTrim(sheet.name), discipline),
    );
    if (descriptor == null) throw _workbookInvalidError();
    final sheet = workbook.sheet(descriptor.name);
    if (sheet == null) throw _workbookInvalidError();
    if (sheet.hasSheetProtection ||
        sheet.maxRow > _sourceFirstDataRow + _maxSourceDataRowsPerSheet) {
      throw _protectedError();
    }
    validateCellText(sheet);
    final rfiCol = findHeaderColumn(sheet, const ['RFI NO.', 'RFI NO']);
    if (rfiCol == null) throw _workbookInvalidError();
    final itpCol = findHeaderColumn(sheet, const ['ITP NO:', 'ITP NO.', 'ITP NO']);
    if (itpCol == null) throw _workbookInvalidError();
    final itemCol = findHeaderColumn(sheet, const ['ITEM NO.', 'ITEM NO']);
    if (itemCol == null) throw _workbookInvalidError();
    final tagCol = findHeaderColumn(sheet, const ['TAG NO', 'TAG NO.']) ?? 6;
    final areaCol = findHeaderColumn(sheet, const ['AREA']) ?? 7;
    final locationCol =
        findHeaderColumn(sheet, const ['INSPECTION LOCATION', 'INSP. LOC./AREA']) ??
        14;
    final statusCol = findHeaderColumn(sheet, const ['INSPECTION STATUS']);
    if (statusCol == null) throw _workbookInvalidError();
    final remarksCol = findHeaderColumn(sheet, const ['REMARKS']);
    if (remarksCol == null) throw _workbookInvalidError();
    for (var row = _sourceFirstDataRow; row <= sheet.maxRow; row++) {
      final rfiCell = sheet.cell(row, rfiCol);
      if (rfiCell == null) continue;
      final canonical = canonicalRfi(rfiCell.value);
      if (canonical == null) continue;
      if (!canonical.contains('-RFI-$discipline-')) continue;
      final shortRfi = canonical.substring(_fullRfiPrefix.length);
      String get(int column) {
        final cell = sheet.cell(row, column);
        return cell == null ? '' : normalizeStatus(cell.value);
      }

      final status = get(statusCol);
      final remarks = get(remarksCol);
      final itp = get(itpCol);
      final item = get(itemCol);
      final tag = get(tagCol);
      final area = get(areaCol);
      final location = get(locationCol);
      final context = SourceRouteContext(tag: tag, area: area, location: location);
      final evidence = FollowUpSourceEvidence(
        sheet: descriptor.name,
        row: row,
        rfiColumn: rfiCol,
        statusColumn: statusCol,
        rfiCell: _cellFromSnapshot(rfiCell),
        statusCell: sheet.cell(row, statusCol),
      );
      final excludedCivPcs = discipline == 'CIV' &&
          splitValues(context.tag).any(
            (tagValue) => normalizeHeader(tagValue).startsWith('PCS-'),
          );
      final incoming = SourceRecord(
        canonicalRfi: canonical,
        shortRfi: shortRfi,
        itp: itp,
        item: item,
        state: logState(status),
        excludedCivPcs: excludedCivPcs,
        worleySigned: mappedFollowUpStatus(remarks) == 'Signed',
        contexts: [context],
        evidence: [evidence],
      );
      final existing = records[canonical];
      if (existing != null) {
        if (existing.excludedCivPcs || incoming.excludedCivPcs) {
          existing.excludedCivPcs = true;
        } else if (existing.itp != incoming.itp ||
            existing.item != incoming.item ||
            existing.state != incoming.state ||
            routeKey(existing) != routeKey(incoming)) {
          existing.state = null;
        } else {
          for (final ctx in incoming.contexts) {
            if (!existing.contexts.contains(ctx)) existing.contexts.add(ctx);
          }
        }
        // An unsigned duplicate is not evidence that an observed signature was withdrawn.
        existing.worleySigned = existing.worleySigned || incoming.worleySigned;
        existing.evidence.addAll(incoming.evidence);
      } else {
        records[canonical] = incoming;
      }
    }
  }
  return records;
}

/// Port of `scan_source` (legacy source scan, all approved status sheets).
Map<String, List<FollowUpSourceEvidence>> scanSource(
  OrderedWorkbookDiscovery discovery,
  WorkbookData workbook,
) {
  if (discovery.safety.hasWorkbookProtection) throw _protectedError();
  final grouped = SplayTreeMap<String, List<FollowUpSourceEvidence>>(
    compareCodePoints,
  );
  final scannedSheets = <String>{};
  for (final projection in followUpStatusColumns) {
    final descriptor = _firstWhereOrNull(
          discovery.worksheets,
          (w) => eqIgnoreAsciiCase(rustTrim(w.name), projection.sheet),
        ) ??
        _firstWhereOrNull(
          discovery.worksheets,
          (w) => matchesDisciplineSheet(w.name, projection.sheet),
        );
    if (descriptor == null) continue;
    final worksheet = workbook.sheet(descriptor.name);
    if (worksheet == null) continue;
    if (!scannedSheets.add(descriptor.name)) continue;
    if (worksheet.hasSheetProtection) throw _protectedError();
    validateCellText(worksheet);
    const lastAllowed = _sourceFirstDataRow + _maxSourceDataRowsPerSheet - 1;
    if (worksheet.maxRow > lastAllowed) throw limitExceededError();
    final columns = detectRfiAndStatusColumns(
      worksheet,
      projection.rfiColumn,
      projection.statusColumn,
    );
    for (var row = 1; row <= worksheet.maxRow; row++) {
      final rfiCell = worksheet.cell(row, columns.$1);
      if (rfiCell == null) continue;
      final rfi = canonicalRfi(rfiCell.value) ??
          canonicalFollowUpRfi(rfiCell.value)?.$1;
      if (rfi == null) continue;
      final statusCell = worksheet.cell(row, columns.$2);
      final evidence = grouped.putIfAbsent(rfi, () => []);
      if (evidence.length >= _maxSourceEvidencePerRfi) throw limitExceededError();
      evidence.add(
        FollowUpSourceEvidence(
          sheet: descriptor.name,
          row: row,
          rfiColumn: columns.$1,
          statusColumn: columns.$2,
          rfiCell: _cellFromSnapshot(rfiCell),
          statusCell: statusCell == null ? null : _cellFromSnapshot(statusCell),
        ),
      );
    }
  }

  if (scannedSheets.isEmpty) {
    // Fallback for consolidated or custom RFI Log workbooks.
    for (final descriptor in discovery.worksheets) {
      if (descriptor.visibility != WorksheetVisibility.visible) continue;
      final worksheet = workbook.sheet(descriptor.name);
      if (worksheet == null) continue;
      if (worksheet.hasSheetProtection) continue;
      final columns = detectRfiAndStatusColumns(worksheet, 1, 20);
      const lastAllowed = _sourceFirstDataRow + _maxSourceDataRowsPerSheet - 1;
      if (worksheet.maxRow > lastAllowed) throw limitExceededError();
      var sheetHadRfis = false;
      for (var row = 1; row <= worksheet.maxRow; row++) {
        final rfiCell = worksheet.cell(row, columns.$1);
        if (rfiCell == null) continue;
        final rfi = canonicalRfi(rfiCell.value) ??
            canonicalFollowUpRfi(rfiCell.value)?.$1;
        if (rfi == null) continue;
        final statusCell = worksheet.cell(row, columns.$2);
        final evidence = grouped.putIfAbsent(rfi, () => []);
        if (evidence.length >= _maxSourceEvidencePerRfi) throw limitExceededError();
        evidence.add(
          FollowUpSourceEvidence(
            sheet: descriptor.name,
            row: row,
            rfiColumn: columns.$1,
            statusColumn: columns.$2,
            rfiCell: _cellFromSnapshot(rfiCell),
            statusCell: statusCell == null ? null : _cellFromSnapshot(statusCell),
          ),
        );
        sheetHadRfis = true;
      }
      if (sheetHadRfis) {
        validateCellText(worksheet);
        scannedSheets.add(descriptor.name);
      }
    }
  }

  if (scannedSheets.isEmpty) {
    throw BackendError.validation(
      'error.rfi.followUp.workbookInvalid: no sheets containing valid RFI entries found in RFI Log',
    );
  }
  return grouped;
}

/// Port of `detect_rfi_and_status_columns`. Returns (rfiColumn, statusColumn).
(int, int) detectRfiAndStatusColumns(
  WorksheetData worksheet,
  int defaultRfiCol,
  int defaultStatusCol,
) {
  final scanRows = min(worksheet.maxRow, 100);
  final hasRfiInDefault = [
    for (var row = 1; row <= scanRows; row++) row,
  ].any((row) {
    final cell = worksheet.cell(row, defaultRfiCol);
    return cell != null &&
        (canonicalRfi(cell.value) != null ||
            canonicalFollowUpRfi(cell.value) != null);
  });
  int rfiColumn;
  if (hasRfiInDefault) {
    rfiColumn = defaultRfiCol;
  } else {
    int? found;
    for (var row = 1; row <= min(10, worksheet.maxRow) && found == null; row++) {
      for (var col = 1;
          col <= min(64, worksheet.maxColumn) && found == null;
          col++) {
        final cell = worksheet.cell(row, col);
        if (cell != null &&
            isFollowUpRfiHeader(normalizeHeader(cell.value))) {
          found = col;
        }
      }
    }
    if (found == null) {
      for (var col = 1; col <= min(64, worksheet.maxColumn) && found == null; col++) {
        for (var row = 1; row <= scanRows; row++) {
          final cell = worksheet.cell(row, col);
          if (cell != null &&
              (canonicalRfi(cell.value) != null ||
                  canonicalFollowUpRfi(cell.value) != null)) {
            found = col;
            break;
          }
        }
      }
    }
    rfiColumn = found ?? defaultRfiCol;
  }

  int? detected;
  for (var row = 1; row <= min(worksheet.maxRow, 10) && detected == null; row++) {
    for (var col = 1; col <= min(worksheet.maxColumn, 64); col++) {
      if (col == rfiColumn) continue;
      final cell = worksheet.cell(row, col);
      if (cell == null) continue;
      final token = normalizeHeader(cell.value);
      if (isFollowUpWorleyStatusHeader(token) ||
          isFollowUpGenericStatusHeader(token) ||
          token == 'NOTLAR' ||
          token == 'AÇIKLAMA' ||
          token == 'ACIKLAMA') {
        detected = col;
        break;
      }
    }
  }
  return (rfiColumn, detected ?? defaultStatusCol);
}

/// Port of `matches_discipline_sheet`.
bool matchesDisciplineSheet(String sheetName, String discipline) {
  final name = rustUpper(rustTrim(sheetName));
  if (name == discipline) return true;
  final tokens = <String>[];
  final buffer = StringBuffer();
  for (final rune in name.runes) {
    final ch = String.fromCharCodes([rune]);
    if (isRustWhitespaceRune(rune) ||
        ch == '-' ||
        ch == '_' ||
        ch == '.' ||
        ch == '/') {
      if (buffer.isNotEmpty) tokens.add(buffer.toString());
      buffer.clear();
    } else {
      buffer.write(ch);
    }
  }
  if (buffer.isNotEmpty) tokens.add(buffer.toString());
  bool hasToken(List<String> patterns) => tokens.any(
    (t) => patterns.any((p) => t == p || t.startsWith(p)),
  );
  switch (discipline) {
    case 'CIV':
      return name.startsWith('CIV') ||
          hasToken(const [
            'CIV',
            'CIVIL',
            'INSAAT',
            'İNŞAAT',
            'STRUCT',
            'STRUCTURAL',
            'CONCRETE',
          ]);
    case 'MEC':
      return name.startsWith('MEC') ||
          hasToken(const [
            'MEC',
            'MECH',
            'MECHANICAL',
            'PIPE',
            'PIPING',
            'MEKANIK',
            'MEKANİK',
            'HVAC',
          ]);
    case 'ELE':
      return name.startsWith('ELE') ||
          hasToken(const [
            'ELE',
            'ELEC',
            'ELECTRICAL',
            'INST',
            'INSTRUMENTATION',
            'ELEKTRIK',
            'ELEKTRİK',
          ]);
    case 'PRC':
      return name.startsWith('PRC') ||
          name.startsWith('PROC') ||
          hasToken(const ['PRC', 'PROC', 'PROCESS', 'PROSES', 'COMMISSIONING']);
    default:
      return false;
  }
}

// ---------------------------------------------------------------------------
// Layout discovery
// ---------------------------------------------------------------------------

/// Port of `discover_approved_layouts` (full-schema path).
List<FollowUpLayout> discoverApprovedLayouts(
  OrderedWorkbookDiscovery discovery,
  WorkbookData workbook,
  bool hasSignature,
  FollowUpMergeBudget mergeBudget,
) {
  if (hasSignature) throw _signedPackageError();
  if (discovery.safety.hasWorkbookProtection) throw _protectedError();
  final layouts = <FollowUpLayout>[];
  for (final descriptor in discovery.worksheets) {
    if (!approvedSheet(descriptor.name)) continue;
    if (descriptor.visibility != WorksheetVisibility.visible) {
      throw _protectedError();
    }
    final sheet = workbook.sheet(descriptor.name);
    if (sheet == null) throw _workbookInvalidError();
    if (sheet.hasSheetProtection) throw _protectedError();
    validateCellText(sheet);
    for (var row = 1; row <= min(sheet.maxRow, 10); row++) {
      for (var rfiColumn = 1;
          rfiColumn < min(sheet.maxColumn, _maxHeaderColumns);
          rfiColumn++) {
        if (rustUpper(rustTrim(descriptor.name)).contains('SIGNED BY WORLEY MEC') &&
            rfiColumn > 14) {
          continue;
        }
        final rfiCell = sheet.cell(row, rfiColumn);
        final rfiHeader = rfiCell == null ? null : normalizeHeader(rfiCell.value);
        final statusCell = sheet.cell(row, rfiColumn + 1);
        final statusHeader =
            statusCell == null ? null : normalizeHeader(statusCell.value);
        if (rfiHeader != null &&
            isFollowUpRfiHeader(rfiHeader) &&
            statusHeader != null &&
            (isFollowUpGenericStatusHeader(statusHeader) ||
                isFollowUpWorleyStatusHeader(statusHeader)) &&
            !mergeBudget.contains(sheet, row, rfiColumn) &&
            !mergeBudget.contains(sheet, row, rfiColumn + 1)) {
          final clause = extractHeaderClause(sheet, row, rfiColumn);
          layouts.add(
            FollowUpLayout(
              worksheet: descriptor,
              headerRow: row,
              rfiColumn: rfiColumn,
              statusColumn: rfiColumn + 1,
              clause: clause,
            ),
          );
        }
      }
    }
    if (descriptor.name == 'PCU') {
      final c318 = sheet.cell(3, 18);
      final c416 = sheet.cell(4, 16);
      final c417 = sheet.cell(4, 17);
      if (c318 != null &&
          normalizeHeader(c318.value) == 'SIGNED BY WORLEY' &&
          c416 != null &&
          isFollowUpRfiHeader(normalizeHeader(c416.value)) &&
          c417 != null &&
          isFollowUpGenericStatusHeader(normalizeHeader(c417.value)) &&
          !mergeBudget.contains(sheet, 4, 16) &&
          !mergeBudget.contains(sheet, 3, 18)) {
        layouts.add(
          FollowUpLayout(
            worksheet: descriptor,
            headerRow: 4,
            rfiColumn: 16,
            statusColumn: 18,
            clause: '3.6',
          ),
        );
      }
    }
  }
  if (layouts.isEmpty) {
    throw BackendError.validation('error.rfi.followUp.layoutMissing');
  }
  return layouts;
}

/// Port of `discover_layouts` (legacy path).
List<FollowUpLayout> discoverLayouts(
  OrderedWorkbookDiscovery discovery,
  WorkbookData workbook,
  bool hasSignature,
  FollowUpMergeBudget mergeBudget,
) {
  if (hasSignature) throw _signedPackageError();
  if (discovery.safety.hasWorkbookProtection) throw _protectedError();
  var candidates = <FollowUpLayout>[];
  final duplicateHeaderSheets = <String>{};
  for (final descriptor in discovery.worksheets) {
    final worksheet = workbook.sheet(descriptor.name);
    if (worksheet == null) throw _workbookInvalidError();
    validateCellText(worksheet);
    for (var row = 1; row <= min(worksheet.maxRow, _maxHeaderRows); row++) {
      final rfiColumns = <(int, bool)>[];
      final statusColumns = <(int, bool)>[];
      for (var column = 1; column <= _maxHeaderColumns; column++) {
        final cell = worksheet.cell(row, column);
        if (cell == null) continue;
        final token = normalizeHeader(cell.value);
        if (token != 'RFI NO.' && token != 'STATUS') continue;
        final safe = cell.formula == null &&
            !mergeBudget.contains(worksheet, row, column);
        if (token == 'RFI NO.') {
          rfiColumns.add((column, safe));
        } else {
          statusColumns.add((column, safe));
        }
      }
      if (rfiColumns.length > 1 || statusColumns.length > 1) {
        duplicateHeaderSheets.add(descriptor.name);
      }
      if (rfiColumns.length == 1 &&
          statusColumns.length == 1 &&
          rfiColumns[0].$2 &&
          statusColumns[0].$2) {
        if (descriptor.visibility != WorksheetVisibility.visible ||
            worksheet.hasSheetProtection) {
          throw _protectedError();
        }
        candidates.add(
          FollowUpLayout(
            worksheet: descriptor,
            headerRow: row,
            rfiColumn: rfiColumns[0].$1,
            statusColumn: statusColumns[0].$1,
          ),
        );
      }
    }
  }
  final sheetCounts = SplayTreeMap<String, int>(compareCodePoints);
  for (final candidate in candidates) {
    sheetCounts[candidate.worksheet.name] =
        (sheetCounts[candidate.worksheet.name] ?? 0) + 1;
  }
  for (final entry in sheetCounts.entries) {
    if (entry.value > 1) duplicateHeaderSheets.add(entry.key);
  }
  candidates = [
    for (final candidate in candidates)
      if (!duplicateHeaderSheets.contains(candidate.worksheet.name)) candidate,
  ];
  if (candidates.isEmpty) {
    if (duplicateHeaderSheets.isNotEmpty) {
      throw BackendError.validation('error.rfi.followUp.layoutAmbiguous');
    }
    throw BackendError.validation('error.rfi.followUp.layoutMissing');
  }
  if (candidates.length == 1) return candidates;
  final preferred = [
    for (final candidate in candidates)
      if (isPreferredFollowUpSheetName(candidate.worksheet.name)) candidate,
  ];
  if (preferred.isNotEmpty) return preferred;
  throw BackendError.validation('error.rfi.followUp.layoutAmbiguous');
}

/// Port of `scan_target_rows`.
List<PendingTargetRow> scanTargetRows(
  WorksheetData worksheet,
  FollowUpLayout layout,
) {
  final lastAllowed = layout.headerRow + _maxFollowUpDataRows;
  if (worksheet.maxRow > lastAllowed) throw limitExceededError();
  final rows = <PendingTargetRow>[];
  for (var row = layout.headerRow + 1; row <= worksheet.maxRow; row++) {
    final rfiCell = worksheet.cell(row, layout.rfiColumn);
    final statusCell = worksheet.cell(row, layout.statusColumn);
    final rawRfi = rfiCell == null ? '' : normalizeStatus(rfiCell.value);
    final canonicalShort = canonicalFollowUpRfi(rawRfi);
    final canonical = canonicalShort?.$1;
    final visible = canonicalShort?.$2 ?? rawRfi;
    final currentStatus =
        statusCell == null ? '' : normalizeStatus(statusCell.value);
    if (visible.isEmpty && currentStatus.isEmpty) continue;
    if (rows.length >= _maxPreviewRows) throw limitExceededError();
    rows.add(
      PendingTargetRow(
        row: row,
        canonicalRfi: canonical,
        visibleRfi: visible,
        currentStatus: currentStatus,
        rfiCell: rfiCell == null ? null : _cellFromSnapshot(rfiCell),
        statusCell: statusCell == null ? null : _cellFromSnapshot(statusCell),
      ),
    );
  }
  return rows;
}

// ---------------------------------------------------------------------------
// Internal helper records and machinery
// ---------------------------------------------------------------------------

typedef SlotKey = (String, int, int, int);

/// Port of `SlotWork`.
class SlotWork {
  SlotWork({
    required this.layout,
    required this.row,
    required this.rfiCell,
    required this.statusCell,
    required this.codes,
    required this.currentStatus,
    required this.changedRfis,
    required this.unsafeReason,
  });

  final FollowUpLayout layout;
  final int row;
  final CellData rfiCell;
  final CellData statusCell;
  List<String> codes;
  final String currentStatus;
  final SplayTreeSet<String> changedRfis;
  final String? unsafeReason;
}

/// Port of `PendingTargetRow`.
class PendingTargetRow {
  const PendingTargetRow({
    required this.row,
    required this.canonicalRfi,
    required this.visibleRfi,
    required this.currentStatus,
    required this.rfiCell,
    required this.statusCell,
  });

  final int row;
  final String? canonicalRfi;
  final String visibleRfi;
  final String currentStatus;
  final CellData? rfiCell;
  final CellData? statusCell;
}

/// Port of `SourceAnalysis`.
class SourceAnalysis {
  const SourceAnalysis({
    required this.statuses,
    required this.proposedStatus,
    required this.reason,
  });

  final List<String> statuses;
  final String? proposedStatus;
  final String? reason;
}

/// Port of `BorrowedFollowUpRow`: borrowed legacy row, materialized on demand.
class BorrowedRow {
  BorrowedRow({
    required this.rowId,
    required this.sheet,
    required this.row,
    required this.rfiNo,
    required this.sourceStatuses,
    required this.currentStatus,
    required this.proposedStatus,
    required this.disposition,
    required this.reasonKey,
    required this.selectable,
    required this.enabled,
    required this.target,
    required this.sourceEvidence,
  });

  final String rowId;
  final String sheet;
  final int row;
  final String rfiNo;
  final List<String> sourceStatuses;
  final String currentStatus;
  final String? proposedStatus;
  final FollowUpDisposition disposition;
  final String reasonKey;
  final bool selectable;
  final bool enabled;
  final FollowUpReviewTarget target;
  final List<FollowUpSourceEvidence> sourceEvidence;

  FollowUpRow materialize() => FollowUpRow(
    rowId: rowId,
    sheet: sheet,
    row: row,
    rfiNo: rfiNo,
    sourceStatuses: sourceStatuses,
    currentStatus: currentStatus,
    proposedStatus: proposedStatus,
    proposedRfiNo: null,
    disposition: disposition,
    reasonKey: reasonKey,
    selectable: selectable,
    enabled: enabled,
    target: target,
    sourceEvidence: sourceEvidence,
    mappingDetails: null,
  );

  JsonObject toBudgetJson() => materialize().toJson()..remove('proposedRfiNo');

}

/// Port of `MergeQueryBudget`.
class FollowUpMergeBudget {
  FollowUpMergeBudget(this.remaining);
  int remaining;

  bool contains(WorksheetData worksheet, int row, int column) {
    final cost = worksheet.mergedRanges.length;
    if (cost > remaining) throw limitExceededError();
    remaining -= cost;
    return worksheet.mergedRanges.any(
      (range) =>
          range.firstRow <= row &&
          row <= range.lastRow &&
          range.firstColumn <= column &&
          column <= range.lastColumn,
    );
  }
}

/// A source record wrapped so that a search result can be typed.
class FollowUpRecord {
  const FollowUpRecord(this.record);
  final SourceRecord record;
}

/// Port of `FollowUpStatusColumns` (mirror of rfi_log `status_columns()`).
class FollowUpStatusColumns {
  const FollowUpStatusColumns(this.sheet, this.rfiColumn, this.statusColumn);
  final String sheet;
  final int rfiColumn;
  final int statusColumn;
}

/// UNVERIFIED mirror of rfi_log::status_columns (CIV 20, MEC 21, ELE 20, PRC 20).
const List<FollowUpStatusColumns> followUpStatusColumns = [
  FollowUpStatusColumns('CIV', 1, 20),
  FollowUpStatusColumns('MEC', 1, 21),
  FollowUpStatusColumns('ELE', 1, 20),
  FollowUpStatusColumns('PRC', 1, 20),
];

final RegExp _fullRfiRe = RegExp(
  r'^AZE-SUN-INT-QMS-RFI-(CIV|MEC|ELE|PRC)-([0-9]{1,4})$',
);
final RegExp _shortRfiRe = RegExp(r'^(?:RFI-)?(CIV|MEC|ELE|PRC)[-_]?([0-9]{1,4})$');
final RegExp _partRe = RegExp(
  r'^(?:(CIV|MEC|ELE|PRC)[-_]?)?([0-9]{1,4})(?:-([0-9]{1,4}))?$',
);
final RegExp _itpRe = RegExp(r'ITP-(CIV|MEC)-([0-9]{4})(?:_[A-Z])?$');
final RegExp _itemRe = RegExp(r'(?:CIV|MEC)-[0-9]+\.([0-9]+\.[0-9]+)$');
final RegExp _itemFullRe = RegExp(r'^(CIV|MEC)-([0-9]+(?:\.[0-9]+)+)$');
final RegExp _clauseParens = RegExp(
  r'\((\d+\.\d+(?:\s*,\s*\d+\.\d+)*)\)',
);
final RegExp _clauseWord = RegExp(r'\b(\d+\.\d+)\b', unicode: true);
final RegExp _digitsOnly = RegExp(r'^[0-9]+$');

// --- rule helpers ----------------------------------------------------------

String? routeKey(SourceRecord record) {
  final route = routeClause(record);
  if (route == null) return null;
  return '${route.$1}\u0000${defaultRouteCol(route.$1, route.$2)}';
}


bool compositePcsRoute(SourceRecord record) => record.contexts.any((context) {
  final tags = normalizedParts(context.tag);
  return tags.where((tag) => tag.startsWith('PCS-')).length == 1 &&
      tags.length > 1;
});

/// Port of `route_clause`.
(String, String)? routeClause(SourceRecord record) {
  final itpId = itpIdentity(record.itp);
  if (itpId == null) return null;
  final clause = itemClause(record.item);
  if (clause == null) return null;
  final pcuTag = record.contexts.any((context) {
    final tags = normalizedParts(context.tag);
    return tags.any((tag) => tag.startsWith('PCS-')) ||
        normalizeHeader(context.tag).startsWith('PCU-');
  });
  final discipline = itpId.$1;
  final itp = itpId.$2;
  switch ((discipline, itp, clause)) {
    case ('CIV', '0002', '1.5'):
      return ('CIV', '1.5');
    case ('CIV', '0002', '1.8'):
      return (pcuTag ? 'PCU' : 'CIV', '1.8');
    case ('CIV', '0002', '2.1') || ('CIV', '0002', '2.2'):
      return ('CIV', '2.1');
    case ('CIV', '0002', '2.4'):
      return (pcuTag ? 'PCU' : 'CIV', '2.4');
    case ('CIV', '0002', '2.5'):
      return (pcuTag ? 'PCU' : 'CIV', '2.5');
    case ('CIV', '0002', '3.1'):
      return ('CIV', '3.1');
    case ('CIV', '0004', '1.7'):
      return ('Road', '1.7');
    case ('CIV', '0004', '2.1'):
      return ('Road', '2.1');
    case ('CIV', '0004', '2.2'):
      return ('Road', '2.2');
    case ('CIV', '0004', '2.3'):
      return ('Road', '2.3');
    case ('CIV', '0004', '2.4'):
      return ('Road', '2.4');
    case ('CIV', '0004', '2.5'):
      return ('Road', '2.5');
    case ('CIV', '0004', '2.6'):
      return ('Road', '2.6');
    case ('CIV', '0004', '2.7'):
      return ('Road', '2.7');
    case ('CIV', '0004', '2.9'):
      return ('Road', '2.9');
    case ('CIV', '0004', '3.2'):
      return ('Road', '3.2');
    case ('CIV', '0005', '2.1'):
      return ('PCU', '2.1');
    case ('CIV', '0005', '2.2'):
      return ('PCU', '2.2');
    case ('CIV', '0005', '2.3'):
      return ('PCU', '2.3');
    case ('CIV', '0005', '3.6'):
      return ('PCU', '3.6');
    case ('MEC', '0001', '1.6'):
      return ('MEC', '1.6');
    case ('MEC', '0001', '2.1') || ('MEC', '0001', '2.2') || ('MEC', '0001', '2.3'):
      return ('MEC', '2.3');
    case ('MEC', '0001', '3.3') || ('MEC', '0001', '3.4'):
      return ('MEC', '3.4');
    case ('MEC', '0001', '3.5'):
      return ('MEC', '3.5');
    case ('MEC', '0003', '3.1'):
      return ('MEC', '3.1');
    case ('MEC', '0004', '3.2'):
      return ('MEC', '3.2');
    default:
      return null;
  }
}

/// Port of `intended_lifecycle_route`.
(String, String)? intendedLifecycleRoute(SourceRecord record) {
  final route = routeClause(record);
  if (route == null) return null;
  if (compositePcsRoute(record)) {
    return ('CIV', switch (route.$2) {
      '1.8' => '1.8',
      '2.4' => '2.4',
      _ => '2.5',
    });
  }
  return route;
}

/// Port of `is_confirmed_follow_up_exclusion` (ADR 0035).
bool isConfirmedFollowUpExclusion(SourceRecord record) {
  final itpId = itpIdentity(record.itp);
  if (itpId == null) return false;
  final itemId = fullItemIdentity(record.item);
  if (itemId == null) return false;
  if (itpId.$1 != itemId.$1) return false;
  final discipline = itpId.$1;
  final itp = itpId.$2;
  final item = itemId.$2;
  return switch ((discipline, itp, item)) {
    ('CIV', '0002', '2.1.4' || '2.1.6') => true,
    ('CIV', '0005', '5.1.4' || '5.1.5' || '5.1.9' || '5.3.4' || '5.3.5') => true,
    ('CIV', '0006', '6.1.8' ||
            '6.2.1' ||
            '6.2.2' ||
            '6.2.3' ||
            '6.2.4' ||
            '6.2.5' ||
            '6.2.9') =>
      true,
    ('CIV', '0007', '7.1.5' || '7.2.3' || '7.2.4' || '7.3.9') => true,
    ('MEC', '0001', '1.1.4' || '1.2.5') => true,
    ('MEC', '0004', '4.3.1') => true,
    _ => false,
  };
}

/// Port of `signed_route_clause`.
(String, String, String)? signedRouteClause(SourceRecord record) {
  final route = routeClause(record);
  if (route == null) return null;
  return switch (route) {
    ('CIV', '3.1') => ('Signed By Worley CIV', '3.1', 'CIV'),
    ('PCU', '3.6') => ('PCU', '3.6', 'PCU'),
    ('MEC', '3.5') => ('Signed By Worley MEC', '3.5', 'MEC'),
    ('MEC', '3.1') => ('Signed By Worley MEC', '3.1', 'MEC'),
    ('MEC', '3.2') => ('Signed By Worley MEC', '3.2', 'MEC'),
    _ => null,
  };
}

/// Port of `default_route_col`.
int defaultRouteCol(String sheet, String clause) {
  return switch ((sheet, clause)) {
    ('CIV', '1.5') => 3,
    ('CIV', '1.8') => 5,
    ('CIV', '2.1') => 7,
    ('CIV', '2.4') => 9,
    ('CIV', '2.5') => 11,
    ('CIV', '3.1') => 13,
    ('PCU', '1.8') => 4,
    ('PCU', '2.4') => 6,
    ('PCU', '2.5') => 8,
    ('PCU', '2.1') => 10,
    ('PCU', '2.2') => 12,
    ('PCU', '2.3') => 14,
    ('PCU', '3.6') => 16,
    ('Road', '1.7') => 3,
    ('Road', '2.1') => 5,
    ('Road', '2.2') => 7,
    ('Road', '2.3') => 9,
    ('Road', '2.4') => 11,
    ('Road', '2.5') => 13,
    ('Road', '2.6') => 15,
    ('Road', '2.7') => 17,
    ('Road', '2.9') => 19,
    ('Road', '3.2') => 23,
    ('MEC', '1.6') => 8,
    ('MEC', '2.3') => 10,
    ('MEC', '3.4') => 12,
    ('MEC', '3.5') => 14,
    ('MEC', '3.1') => 16,
    ('MEC', '3.2') => 18,
    _ => 3,
  };
}

/// Port of `default_signed_col`.
(int, int) defaultSignedCol(String sheet, String clause) {
  return switch ((sheet, clause)) {
    ('Signed By Worley CIV', '3.1') => (3, 4),
    ('PCU', '3.6') => (16, 18),
    ('Signed By Worley MEC', '3.5') => (8, 9),
    ('Signed By Worley MEC', '3.1') => (10, 11),
    ('Signed By Worley MEC', '3.2') => (12, 13),
    _ => (3, 4),
  };
}

/// Port of `is_signed_layout`.
bool isSignedLayout(FollowUpLayout layout) {
  final name = rustUpper(rustTrim(layout.worksheet.name));
  return name.contains('SIGNED BY WORLEY') ||
      (name.contains('PCU') && layout.statusColumn == 18);
}

/// Port of `normalized_location`.
String normalizedLocation(String value) => normalizeHeader(value)
    .replaceAll(' SITE', '')
    .replaceAll('PCU-', 'PCS-')
    .replaceAll('ROAD 0', 'ROAD ');

/// Port of `normalized_parts`.
List<String> normalizedParts(String value) => [
  for (final part in splitValues(value).map(normalizedLocation))
    if (part.isNotEmpty) part,
];

/// Port of `split_values`.
List<String> splitValues(String value) {
  final parts = <String>[];
  final buffer = StringBuffer();
  for (final ch in value.split('')) {
    if (ch == ',' || ch == '/' || ch == ';' || ch == '\n') {
      parts.add(buffer.toString());
      buffer.clear();
    } else {
      buffer.write(ch);
    }
  }
  parts.add(buffer.toString());
  return [
    for (final part in parts)
      if (normalizeStatus(part).isNotEmpty) normalizeStatus(part),
  ];
}

/// Port of `pcu_row_matches`.
bool pcuRowMatches(WorksheetData sheet, int row, SourceRouteContext context) {
  final tags = normalizedParts(context.tag);
  final blocks = [for (final tag in tags) if (tag.startsWith('PCS-')) tag];
  String block;
  if (blocks.length == 1) {
    block = blocks[0];
  } else if (blocks.isEmpty && normalizeHeader(context.tag).startsWith('PCU-')) {
    block = normalizedLocation(context.tag);
  } else {
    return false;
  }
  final blockCell = sheet.cell(row, 3);
  if (blockCell == null || normalizedLocation(blockCell.value) != block) {
    return false;
  }
  final areas = [for (final tag in tags) if (!tag.startsWith('PCS-')) tag];
  if (areas.isEmpty) return true;
  final areaCell = sheet.cell(row, 2);
  if (areaCell == null) return false;
  return normalizedParts(areaCell.value).any(areas.contains);
}

/// Port of `unique_zone_for_pcs`.
String? uniqueZoneForPcs(WorkbookData targetWorkbook, String pcs) {
  final sheet = targetWorkbook.sheet('MEC');
  if (sheet == null) return null;
  final zones = <String>{};
  for (var row = 1; row <= sheet.maxRow; row++) {
    final block = sheet.cell(row, 3);
    if (block == null) continue;
    if (normalizedLocation(block.value) != pcs) continue;
    final zoneCell = sheet.cell(row, 1);
    final zone = zoneCell == null ? '' : normalizedLocation(zoneCell.value);
    if (zone.isNotEmpty) zones.add(zone);
  }
  return zones.length == 1 ? zones.first : null;
}

/// Port of `row_matches` (any source context).
bool rowMatches(
  String sheetName,
  WorksheetData sheet,
  int row,
  SourceRecord record,
  WorkbookData targetWorkbook,
) => record.contexts.any(
  (context) => rowMatchesContext(sheetName, sheet, row, context, targetWorkbook),
);

/// Port of `row_matches_context`.
bool rowMatchesContext(
  String sheetName,
  WorksheetData sheet,
  int row,
  SourceRouteContext context,
  WorkbookData targetWorkbook,
) {
  String cell(int column) {
    final c = sheet.cell(row, column);
    return c == null ? '' : normalizedLocation(c.value);
  }

  switch (sheetName) {
    case 'CIV':
      final tags = normalizedParts(context.tag);
      final pcs = [for (final tag in tags) if (tag.startsWith('PCS-')) tag];
      if (pcs.length == 1 && tags.length > 1) {
        final areas = <String>{
          ...tags.where((tag) => !tag.startsWith('PCS-')),
          ...normalizedParts(context.area),
        };
        final zone = uniqueZoneForPcs(targetWorkbook, pcs[0]);
        return zone != null && zone == cell(1) && areas.contains(cell(2));
      } else if (pcs.isEmpty) {
        return tags.any((value) => value == cell(2));
      }
      return false;
    case 'PCU':
      return pcuRowMatches(sheet, row, context);
    case 'Road':
      return normalizedLocation(context.tag) == cell(2);
    case 'MEC':
      final targetSections = normalizedParts(cell(4));
      final targetTags = normalizedParts(cell(3));
      final sections = normalizedParts(context.area);
      final tags = normalizedParts(context.tag);
      return sections.isNotEmpty &&
          tags.isNotEmpty &&
          tags.any(targetTags.contains) &&
          sections.any(
            (value) => targetSections.contains(normalizedLocation(value)),
          );
    default:
      return false;
  }
}

/// Port of `record_matches_lifecycle_slot`.
bool recordMatchesLifecycleSlot(
  SlotWork slot,
  SourceRecord record,
  WorkbookData targetWorkbook,
) {
  final route = intendedLifecycleRoute(record);
  if (route == null) return true;
  final routeSheet = route.$1;
  final routeClauseName = route.$2;
  if (!eqIgnoreAsciiCase(rustTrim(slot.layout.worksheet.name), routeSheet)) {
    return false;
  }
  final layoutClause = slot.layout.clause;
  if (layoutClause != null && layoutClause != routeClauseName) return false;
  if (layoutClause == null &&
      slot.layout.rfiColumn != defaultRouteCol(routeSheet, routeClauseName)) {
    return false;
  }
  final sheet = targetWorkbook.sheet(slot.layout.worksheet.name) ??
      findSheetFlexible(targetWorkbook, slot.layout.worksheet.name);
  if (sheet == null) return false;
  return rowMatches(routeSheet, sheet, slot.row, record, targetWorkbook);
}

/// Port of `signed_destination_coverage`.
({bool valid, bool missingContexts}) signedDestinationCoverage(
  String sheetName,
  WorksheetData sheet,
  List<SlotKey> candidates,
  SourceRecord record,
) {
  const invalid = (valid: false, missingContexts: false);
  if (sheetName == 'Signed By Worley MEC' &&
      record.evidence.length > record.contexts.length) {
    return invalid;
  }
  if (sheetName == 'Signed By Worley CIV') {
    final sourceTags = <String>{
      for (final context in record.contexts)
        ...normalizedParts(context.tag),
    }..removeWhere((tag) => tag.startsWith('PCS-'));
    if (sourceTags.isEmpty) return invalid;
    final matchedTags = <String>{};
    for (final key in candidates) {
      final tagCell = sheet.cell(key.$2, 2);
      final tag = tagCell == null ? '' : normalizedLocation(tagCell.value);
      if (!sourceTags.contains(tag) || !matchedTags.add(tag)) return invalid;
    }
    return (valid: true, missingContexts: matchedTags.length < sourceTags.length);
  }
  if (sheetName == 'Signed By Worley MEC') {
    final sourcePairs = <(String, String)>{};
    for (final context in record.contexts) {
      final tags = normalizedParts(context.tag);
      final blocks = [for (final tag in tags) if (tag.startsWith('PCS-')) tag];
      final areas = normalizedParts(context.area);
      if (tags.length != 1 || blocks.length != 1 || areas.isEmpty) return invalid;
      for (final area in areas) {
        sourcePairs.add((blocks[0], area));
      }
    }
    if (sourcePairs.isEmpty) return invalid;
    final matchedPairs = <(String, String)>{};
    for (final key in candidates) {
      final blockCell = sheet.cell(key.$2, 3);
      final areaCell = sheet.cell(key.$2, 4);
      final blocks = blockCell == null ? <String>[] : normalizedParts(blockCell.value);
      final areas = areaCell == null ? <String>[] : normalizedParts(areaCell.value);
      if (blocks.length != 1 || areas.length != 1) return invalid;
      final matching = [
        for (final pair in sourcePairs)
          if (blocks.contains(pair.$1) && areas.contains(pair.$2)) pair,
      ];
      if (matching.length != 1) return invalid;
      if (!matchedPairs.add(matching.first)) return invalid;
    }
    return (valid: true, missingContexts: matchedPairs.length < sourcePairs.length);
  }
  return (valid: candidates.length == 1, missingContexts: false);
}

/// Port of `civ_tag_coverage`.
bool? civTagCoverage(
  WorkbookData targetWorkbook,
  List<SlotKey> candidates,
  SourceRecord record,
) {
  final sourceTags = <String>{
    for (final context in record.contexts) ...normalizedParts(context.tag),
  }..removeWhere((tag) => tag.startsWith('PCS-'));
  if (candidates.isEmpty || sourceTags.length < 2) return null;
  final sheet = targetWorkbook.sheet('CIV');
  if (sheet == null) return null;
  final matchedTags = <String>{};
  for (final key in candidates) {
    final cell = sheet.cell(key.$2, 2);
    final tags = cell == null ? <String>[] : normalizedParts(cell.value);
    if (tags.length != 1 ||
        !sourceTags.contains(tags[0]) ||
        !matchedTags.add(tags[0])) {
      return null;
    }
  }
  return matchedTags.length == sourceTags.length;
}

/// Port of `distinct_civ_area_rows`.
bool distinctCivAreaRows(
  WorkbookData targetWorkbook,
  List<SlotKey> candidates,
  SourceRecord record,
) {
  if (candidates.length < 2) return false;
  final sheet = targetWorkbook.sheet('CIV');
  if (sheet == null) return false;
  final sourceTags = <String>{
    for (final context in record.contexts)
      for (final value in splitValues(context.tag)) normalizedLocation(value),
  };
  final matchedTags = <String>{};
  return candidates.every((key) {
    final cell = sheet.cell(key.$2, 2);
    final tag = cell == null ? '' : normalizedLocation(cell.value);
    return tag.isNotEmpty && sourceTags.contains(tag) && matchedTags.add(tag);
  });
}

/// Port of `distinct_civ_zone_area_rows`.
bool distinctCivZoneAreaRows(
  WorkbookData targetWorkbook,
  List<SlotKey> candidates,
  SourceRecord record,
) {
  if (candidates.length < 2) return false;
  final sheet = targetWorkbook.sheet('CIV');
  if (sheet == null) return false;
  final contexts = <(String, Set<String>)>[];
  for (final context in record.contexts) {
    final tags = normalizedParts(context.tag);
    final pcs = [for (final tag in tags) if (tag.startsWith('PCS-')) tag];
    final areas = <String>{
      ...tags.where((tag) => !tag.startsWith('PCS-')),
      ...normalizedParts(context.area),
    };
    final zone = pcs.length == 1 ? uniqueZoneForPcs(targetWorkbook, pcs[0]) : null;
    if (areas.isNotEmpty && zone != null) contexts.add((zone, areas));
  }
  if (contexts.isEmpty) return false;
  final matched = <(int, String, String)>{};
  return candidates.every((key) {
    final zoneCell = sheet.cell(key.$2, 1);
    final areaCell = sheet.cell(key.$2, 2);
    final zone = zoneCell == null ? '' : normalizedLocation(zoneCell.value);
    final area = areaCell == null ? '' : normalizedLocation(areaCell.value);
    return contexts.any((ctx) => ctx.$1 == zone && ctx.$2.contains(area)) &&
        matched.add((key.$2, zone, area));
  });
}

/// Port of `mec_context_coverage`.
bool? mecContextCoverage(
  WorkbookData targetWorkbook,
  String sheetName,
  List<SlotKey> candidates,
  SourceRecord record,
) {
  if (candidates.isEmpty) return null;
  final sheet = targetWorkbook.sheet(sheetName);
  if (sheet == null) return null;
  final sourceBlocks = <String>{};
  final sourceSections = <String>{};
  final sourceContexts = <(String, String)>{};
  for (final context in record.contexts) {
    final blocks = [
      for (final tag in normalizedParts(context.tag))
        if (tag.startsWith('PCS-')) tag,
    ];
    final areas = [
      for (final area in normalizedParts(context.area))
        if (area.startsWith('S')) area,
    ];
    if (blocks.length != 1 || areas.isEmpty) return null;
    sourceBlocks.add(blocks[0]);
    for (final area in areas) {
      sourceSections.add(area);
      sourceContexts.add((blocks[0], area));
    }
  }
  if (sourceBlocks.length != 1 ||
      sourceSections.isEmpty ||
      sourceContexts.length < 2) {
    return null;
  }
  final matchedContexts = <(String, String)>{};
  for (final key in candidates) {
    final blockCell = sheet.cell(key.$2, 3);
    final areaCell = sheet.cell(key.$2, 4);
    final blocks = blockCell == null ? <String>[] : normalizedParts(blockCell.value);
    final areas = areaCell == null ? <String>[] : normalizedParts(areaCell.value);
    if (blocks.length != 1 || areas.length != 1) return null;
    final context = (blocks[0], areas[0]);
    if (!sourceBlocks.contains(context.$1) ||
        !sourceSections.contains(context.$2) ||
        !sourceContexts.contains(context) ||
        !matchedContexts.add(context)) {
      return null;
    }
  }
  return matchedContexts.length == sourceContexts.length;
}

/// Port of `safe_slot`.
SlotWork? safeSlot(
  FollowUpLayout layout,
  WorksheetData sheet,
  int row,
  FollowUpMergeBudget mergeBudget,
) {
  final rfiCell = sheet.cell(row, layout.rfiColumn);
  if (rfiCell == null) return null;
  final statusCell = sheet.cell(row, layout.statusColumn);
  if (statusCell == null) return null;
  String? unsafeReason;
  if (rfiCell.formula != null) {
    unsafeReason = 'targetRfiFormula';
  } else if (statusCell.formula != null) {
    unsafeReason = 'targetFormula';
  } else if (mergeBudget.contains(sheet, row, layout.rfiColumn)) {
    unsafeReason = 'targetRfiMerged';
  } else if (mergeBudget.contains(sheet, row, layout.statusColumn)) {
    unsafeReason = 'targetMerged';
  }
  final raw = normalizeStatus(rfiCell.value);
  List<String> codes;
  if (raw.isEmpty) {
    codes = [];
  } else {
    final parsed = parseRfiGroup(raw);
    if (parsed == null) {
      return SlotWork(
        layout: layout,
        row: row,
        rfiCell: _cellFromSnapshot(rfiCell),
        statusCell: _cellFromSnapshot(statusCell),
        codes: [],
        currentStatus: normalizeStatus(statusCell.value),
        changedRfis: SplayTreeSet<String>(compareCodePoints),
        unsafeReason: 'invalidRfi',
      );
    }
    codes = parsed;
  }
  return SlotWork(
    layout: layout,
    row: row,
    rfiCell: _cellFromSnapshot(rfiCell),
    statusCell: _cellFromSnapshot(statusCell),
    codes: codes,
    currentStatus: normalizeStatus(statusCell.value),
    changedRfis: SplayTreeSet<String>(compareCodePoints),
    unsafeReason: unsafeReason,
  );
}

/// Port of `main_status_for_codes`.
String? mainStatusForCodes(List<String> codes, SplayTreeMap<String, SourceRecord> records) {
  final real = [for (final code in codes) if (!isPlaceholderRfi(code)) code];
  if (real.isEmpty) return null;
  final states = <LogState>[];
  for (final code in real) {
    final state = records[code]?.state;
    if (state == null) return null;
    states.add(state);
  }
  return aggregateStatus(states);
}

/// Port of `row_from_slot`.
FollowUpRow? rowFromSlot(
  SlotWork slot,
  SplayTreeMap<String, SourceRecord> records, {
  required bool signed,
}) {
  if (slot.changedRfis.isEmpty || slot.unsafeReason != null) return null;
  final proposedRfi = formatRfiGroup(slot.codes) ?? '';
  String proposedStatus;
  if (signed) {
    if (slot.codes.isEmpty) {
      proposedStatus = '';
    } else {
      final real = [for (final code in slot.codes) if (!isPlaceholderRfi(code)) code];
      if (real.isEmpty) {
        proposedStatus = slot.currentStatus;
      } else {
        final states = <SourceRecord>[];
        for (final code in real) {
          final record = records[code];
          if (record == null) return null;
          states.add(record);
        }
        if (states.isEmpty) return null;
        proposedStatus = states.every((record) => record.worleySigned)
            ? 'Signed'
            : 'Not Signed';
      }
    }
  } else if (slot.codes.isEmpty) {
    proposedStatus = '';
  } else {
    proposedStatus =
        mainStatusForCodes(slot.codes, records) ?? slot.currentStatus;
  }
  if (proposedRfi == slot.rfiCell.value && proposedStatus == slot.currentStatus) {
    return null;
  }
  final canonical = slot.changedRfis.first;
  final record = records[canonical];
  if (record == null) return null;
  final layout = slot.layout;
  final rowId = layout.statusColumn == layout.rfiColumn + 1
      ? 'rfi-follow-up:${layout.worksheet.sheetId}:${slot.row}:${layout.rfiColumn}'
      : 'rfi-follow-up:${layout.worksheet.sheetId}:${slot.row}:${layout.rfiColumn}:${layout.statusColumn}';
  final String reason;
  if (record.excludedCivPcs || record.state == LogState.remove) {
    reason = 'warn.rfi.followUp.reason.readyRemove';
  } else if (signed) {
    reason = 'warn.rfi.followUp.reason.readyWorleySync';
  } else {
    reason = 'warn.rfi.followUp.reason.readyLogSync';
  }
  return FollowUpRow(
    rowId: rowId,
    sheet: layout.worksheet.name,
    row: slot.row,
    rfiNo: record.shortRfi,
    sourceStatuses: [
      signed
          ? (record.worleySigned ? 'Signed' : 'Not Signed')
          : record.inspectionStatusForUi(),
    ],
    currentStatus: slot.currentStatus,
    proposedStatus: proposedStatus,
    proposedRfiNo: proposedRfi,
    disposition: FollowUpDisposition.ready,
    reasonKey: reason,
    selectable: true,
    enabled: true,
    target: FollowUpReviewTarget(
      worksheet: layout.worksheet,
      row: slot.row,
      rfiColumn: layout.rfiColumn,
      statusColumn: layout.statusColumn,
      rfiCell: slot.rfiCell,
      statusCell: slot.statusCell,
    ),
    sourceEvidence: record.evidence,
    mappingDetails: null,
  );
}

/// Port of `unsafe_slot_review`.
FollowUpRow? unsafeSlotReview(
  SlotWork slot,
  SplayTreeMap<String, SourceRecord> records,
) {
  final reason = slot.unsafeReason;
  if (reason == null) return null;
  SourceRecord? record;
  for (final code in slot.changedRfis) {
    final found = records[code];
    if (found != null) {
      record = found;
      break;
    }
  }
  if (record == null) return null;
  final layout = slot.layout;
  return FollowUpRow(
    rowId:
        'rfi-follow-up-removal-unsafe:${layout.worksheet.sheetId}:${slot.row}:${layout.rfiColumn}',
    sheet: layout.worksheet.name,
    row: slot.row,
    rfiNo: record.shortRfi,
    sourceStatuses: [record.inspectionStatusForUi()],
    currentStatus: slot.currentStatus,
    proposedStatus: null,
    proposedRfiNo: null,
    disposition: FollowUpDisposition.manualReview,
    reasonKey: 'warn.rfi.followUp.reason.$reason',
    selectable: false,
    enabled: false,
    target: FollowUpReviewTarget(
      worksheet: layout.worksheet,
      row: slot.row,
      rfiColumn: layout.rfiColumn,
      statusColumn: layout.statusColumn,
      rfiCell: slot.rfiCell,
      statusCell: slot.statusCell,
    ),
    sourceEvidence: record.evidence,
    mappingDetails: null,
  );
}

/// Port of `missing_final_review`.
FollowUpRow missingFinalReview(
  SourceRecord record,
  FollowUpLayout layout,
  String sheet,
  String targetClause,
  (int, int) columns,
  List<SlotKey> candidates,
  WorkbookData targetWorkbook,
) {
  return FollowUpRow(
    rowId: 'rfi-follow-up-unmapped:${record.canonicalRfi}',
    sheet: sheet,
    row: 0,
    rfiNo: record.shortRfi,
    sourceStatuses: [record.worleySigned ? 'Signed' : 'Not Signed'],
    currentStatus: '',
    proposedStatus: null,
    proposedRfiNo: null,
    disposition: FollowUpDisposition.manualReview,
    reasonKey: 'warn.rfi.followUp.reason.mappingMissing',
    selectable: false,
    enabled: false,
    target: FollowUpReviewTarget(
      worksheet: layout.worksheet,
      row: 0,
      rfiColumn: columns.$1,
      statusColumn: columns.$2,
    ),
    sourceEvidence: record.evidence,
    mappingDetails: mappingDetails(
      record,
      (sheet, targetClause),
      targetWorkbook,
      candidates,
    ),
  );
}

FollowUpRow _withReason(FollowUpRow row, String reason) => FollowUpRow(
  rowId: row.rowId,
  sheet: row.sheet,
  row: row.row,
  rfiNo: row.rfiNo,
  sourceStatuses: row.sourceStatuses,
  currentStatus: row.currentStatus,
  proposedStatus: row.proposedStatus,
  proposedRfiNo: row.proposedRfiNo,
  disposition: row.disposition,
  reasonKey: reason,
  selectable: row.selectable,
  enabled: row.enabled,
  target: row.target,
  sourceEvidence: row.sourceEvidence,
  mappingDetails: row.mappingDetails,
);

/// Port of `safe_slot`-adjacent `slot_compatible` check used by both the
/// lifecycle and FINAL routes.
bool slotCompatibleWith(
  SlotWork slot,
  SourceRecord record,
  SplayTreeMap<String, SourceRecord> records,
) {
  return slot.codes
      .where((code) => !isPlaceholderRfi(code))
      .every((code) {
    final existing = records[code];
    return existing != null &&
        existing.itp == record.itp &&
        itemClause(existing.item) == itemClause(record.item);
  });
}

String? _firstSourceIssue(
  SlotWork slot,
  SplayTreeMap<String, SourceRecord> records,
) {
  for (final code in slot.codes) {
    if (isPlaceholderRfi(code)) continue;
    final record = records[code];
    if (record == null) return 'sourceMissing';
    if (record.state == null) return 'sourceStatusUnknown';
  }
  return null;
}

FollowUpLayout _slotLayout(
  SplayTreeMap<SlotKey, SlotWork> slots,
  List<SlotKey> candidates,
) {
  final slot = slots[candidates.first];
  if (slot == null) throw _workbookInvalidError();
  return slot.layout;
}

/// Port of `classify_borrowed_row`.
BorrowedRow classifyBorrowedRow(
  PendingTargetRow pending,
  FollowUpLayout layout,
  WorksheetData targetWorksheet,
  List<FollowUpSourceEvidence> evidence,
  SourceAnalysis analysis,
  Map<String, int> duplicateCounts,
  FollowUpMergeBudget mergeBudget,
) {
  final rfi = pending.canonicalRfi;
  final duplicate = rfi != null && (duplicateCounts[rfi] ?? 0) > 1;
  final rfiFormula = pending.rfiCell?.formula != null;
  final rfiMerged = mergeBudget.contains(targetWorksheet, pending.row, layout.rfiColumn);
  final statusFormula = pending.statusCell?.formula != null;
  final statusMerged =
      mergeBudget.contains(targetWorksheet, pending.row, layout.statusColumn);

  final (FollowUpDisposition disposition, String reason) = () {
    if (rfiFormula) return _manual('targetRfiFormula');
    if (rfiMerged) return _manual('targetRfiMerged');
    if (pending.visibleRfi.isEmpty) return _manual('blankRfi');
    if (pending.canonicalRfi == null) return _manual('invalidRfi');
    if (analysis.proposedStatus == pending.currentStatus) {
      return (
        FollowUpDisposition.alreadySynced,
        'warn.rfi.followUp.reason.alreadySynced',
      );
    }
    if (duplicate) return _manual('targetDuplicate');
    if (evidence.isEmpty) return _manual('sourceMissing');
    final analysisReason = analysis.reason;
    if (analysisReason != null) return _manual(analysisReason);
    if (pending.statusCell == null) return _manual('targetCellMissing');
    if (statusFormula) return _manual('targetFormula');
    if (statusMerged) return _manual('targetMerged');
    if (pending.currentStatus.isNotEmpty &&
        pending.currentStatus != 'Signed' &&
        pending.currentStatus != 'Not Signed') {
      return _manual('targetStatusUnknown');
    }
    return (
      FollowUpDisposition.ready,
      analysis.proposedStatus == 'Signed'
          ? 'warn.rfi.followUp.reason.readySigned'
          : 'warn.rfi.followUp.reason.readyNotSigned',
    );
  }();
  final selectable = disposition == FollowUpDisposition.ready;
  return BorrowedRow(
    rowId:
        'rfi-follow-up:${layout.worksheet.sheetId}:${pending.row}:${layout.statusColumn}',
    sheet: layout.worksheet.name,
    row: pending.row,
    rfiNo: pending.visibleRfi,
    sourceStatuses: analysis.statuses,
    currentStatus: pending.currentStatus,
    proposedStatus: analysis.proposedStatus,
    disposition: disposition,
    reasonKey: reason,
    selectable: selectable,
    enabled: selectable,
    target: FollowUpReviewTarget(
      worksheet: layout.worksheet,
      row: pending.row,
      rfiColumn: layout.rfiColumn,
      statusColumn: layout.statusColumn,
      rfiCell: pending.rfiCell,
      statusCell: pending.statusCell,
    ),
    sourceEvidence: evidence,
  );
}

(FollowUpDisposition, String) _manual(String reason) =>
    (FollowUpDisposition.manualReview, 'warn.rfi.followUp.reason.$reason');

/// Port of `analyze_source`.
SourceAnalysis analyzeSource(
  List<FollowUpSourceEvidence> evidence,
  WorkbookData workbook,
  FollowUpMergeBudget mergeBudget,
) {
  followUpSourceAnalysisCalls += 1;
  final statuses = [
    ...{
      for (final item in evidence)
        mappedFollowUpStatus(item.statusCell?.value ?? ''),
    },
  ]..sort(compareCodePoints);
  if (evidence.isEmpty) {
    return SourceAnalysis(statuses: statuses, proposedStatus: null, reason: null);
  }
  if (evidence.any(
    (item) => item.rfiCell.formula != null || item.statusCell?.formula != null,
  )) {
    return SourceAnalysis(
      statuses: statuses,
      proposedStatus: null,
      reason: 'sourceFormula',
    );
  }
  var sourceMerged = false;
  for (final item in evidence) {
    final worksheet = workbook.sheet(item.sheet);
    if (worksheet == null) continue;
    if (mergeBudget.contains(worksheet, item.row, item.rfiColumn) ||
        mergeBudget.contains(worksheet, item.row, item.statusColumn)) {
      sourceMerged = true;
      break;
    }
  }
  if (sourceMerged) {
    return SourceAnalysis(
      statuses: statuses,
      proposedStatus: null,
      reason: 'sourceMerged',
    );
  }
  if (statuses.length == 1) {
    return SourceAnalysis(
      statuses: statuses,
      proposedStatus: statuses.first,
      reason: null,
    );
  }
  return SourceAnalysis(
    statuses: statuses,
    proposedStatus: null,
    reason: 'sourceConflict',
  );
}

/// Port of `PendingTargetRow` consumers: sheets/row id collation helpers.
bool approvedSheet(String name) {
  final norm = rustUpper(rustTrim(name));
  return const {
    'CIV',
    'MEC',
    'PCU',
    'ROAD',
    'SIGNED BY WORLEY CIV',
    'SIGNED BY WORLEY MEC',
  }.contains(norm);
}

/// Port of `extract_header_clause`.
String? extractHeaderClause(WorksheetData sheet, int maxRow, int column) {
  for (var r = maxRow - 1; r >= 1; r--) {
    final cell = sheet.cell(r, column);
    if (cell == null) continue;
    final text = cell.value;
    final parens = _clauseParens.firstMatch(text);
    if (parens != null) {
      final last = parens.group(1)!.split(',').last;
      return rustTrim(last);
    }
    final word = _clauseWord.firstMatch(text);
    if (word != null) return rustTrim(word.group(1)!);
  }
  return null;
}

/// Port of `is_preferred_follow_up_sheet_name`.
bool isPreferredFollowUpSheetName(String name) {
  final upper = asciiUpper(name);
  final trimmed = rustTrim(upper);
  return trimmed.contains('SIGNED BY') ||
      trimmed.contains('FOLLOW UP') ||
      trimmed.contains('FOLLOW-UP') ||
      trimmed.contains('FOLLOWUP') ||
      trimmed == 'CIV' ||
      trimmed == 'MEC' ||
      trimmed == 'ELE' ||
      trimmed == 'PRC' ||
      trimmed.startsWith('CIV ') ||
      trimmed.startsWith('MEC ') ||
      trimmed.startsWith('ELE ') ||
      trimmed.startsWith('PRC ') ||
      trimmed.endsWith(' CIV') ||
      trimmed.endsWith(' MEC') ||
      trimmed.endsWith(' ELE') ||
      trimmed.endsWith(' PRC') ||
      trimmed.endsWith('-CIV') ||
      trimmed.endsWith('-MEC') ||
      trimmed.endsWith('-ELE') ||
      trimmed.endsWith('-PRC');
}

/// Port of `find_header_column`.
int? findHeaderColumn(WorksheetData sheet, List<String> names) {
  for (var row = 1; row <= min(sheet.maxRow, 16); row++) {
    for (var column = 1; column <= min(sheet.maxColumn, _maxHeaderColumns); column++) {
      final cell = sheet.cell(row, column);
      if (cell == null) continue;
      if (names.contains(normalizeHeader(cell.value))) return column;
    }
  }
  return null;
}

/// Port of `find_sheet_flexible`. Sheets are visited in sorted name order.
WorksheetData? findSheetFlexible(WorkbookData workbook, String target) {
  final exact = workbook.sheet(target);
  if (exact != null) return exact;
  final norm = rustUpper(rustTrim(target));
  final names = workbook.sheets.keys.toList()..sort(compareCodePoints);
  for (final name in names) {
    if (rustUpper(rustTrim(name)) == norm) return workbook.sheets[name];
  }
  return null;
}

/// Port of `is_follow_up_rfi_header`.
bool isFollowUpRfiHeader(String token) =>
    token == 'RFI NO.' ||
    token == 'RFI NO' ||
    token == 'RFI #' ||
    token == 'RFI NUMBER' ||
    token == 'RFI NO:' ||
    token == 'RFI REF' ||
    token == 'RFI REFERENCE';

/// Port of `is_follow_up_worley_status_header`.
bool isFollowUpWorleyStatusHeader(String token) =>
    token == 'WORLEY STATUS' ||
    token == 'WORLEY SIGNATURE' ||
    token == 'WORLEY REMARKS' ||
    token == 'WORLEY REMARK' ||
    token == 'STATUS (WORLEY)' ||
    token == 'SIGNATURE (WORLEY)';

/// Port of `is_follow_up_generic_status_header`.
bool isFollowUpGenericStatusHeader(String token) =>
    token == 'STATUS' ||
    token == 'RFI STATUS' ||
    token == 'CURRENT STATUS' ||
    token == 'SIGNATURE' ||
    token == 'SIGNATURE STATUS' ||
    token == 'DURUM' ||
    token == 'REMARKS' ||
    token == 'REMARK';

/// Port of `validate_cell_text`.
void validateCellText(WorksheetData sheet) {
  for (final cell in sheet.cells.values) {
    if (_runeCount(cell.value) > _maxCellTextChars ||
        (cell.formula != null && _runeCount(cell.formula!) > _maxCellTextChars)) {
      throw limitExceededError();
    }
  }
}

/// Port of `mapping_details`.
String mappingDetails(
  SourceRecord record,
  (String, String)? route,
  WorkbookData targetWorkbook,
  List<SlotKey> candidates,
) {
  var details = 'ITP: ${compactDetail(record.itp)}; item: ${compactDetail(record.item)}';
  final ignoreInspectionLocation =
      route != null && (route.$1 == 'MEC' || route.$1 == 'Signed By Worley MEC');
  String contextLabel(SourceRouteContext context) {
    final tag = compactDetail(context.tag);
    final area = compactDetail(context.area);
    if (ignoreInspectionLocation) return 'tag=$tag; area=$area';
    return 'tag=$tag; area=$area; location=${compactDetail(context.location)}';
  }

  final splitCivTags =
      route != null && (route.$1 == 'CIV' || route.$1 == 'Signed By Worley CIV');
  final List<SourceRouteContext> sourceContexts;
  if (ignoreInspectionLocation || splitCivTags) {
    sourceContexts = [];
    for (final context in record.contexts) {
      final values = ignoreInspectionLocation
          ? normalizedParts(context.area)
          : [
              for (final tag in normalizedParts(context.tag))
                if (!tag.startsWith('PCS-')) tag,
            ];
      if (values.isEmpty) {
        sourceContexts.add(context);
      } else {
        for (final value in values) {
          sourceContexts.add(
            ignoreInspectionLocation
                ? SourceRouteContext(
                    tag: context.tag,
                    area: value,
                    location: context.location,
                  )
                : SourceRouteContext(
                    tag: value,
                    area: context.area,
                    location: context.location,
                  ),
          );
        }
      }
    }
  } else {
    sourceContexts = [...record.contexts];
  }
  final contexts = SplayTreeSet<String>(compareCodePoints)
    ..addAll(sourceContexts.map(contextLabel));
  if (contexts.isEmpty) {
    details += '; source context: none';
  } else {
    details += '; source context: ${contexts.take(5).join(' | ')}';
    if (contexts.length > 5) details += ' | +${contexts.length - 5} more';
  }
  if (route != null) {
    details += '; expected target: ${route.$1}/${route.$2}';
  } else {
    details += '; expected target: no configured Follow Up route';
  }
  if (candidates.isEmpty) {
    details += '; matching prepared target rows: none';
  } else {
    final targetColumns = ignoreInspectionLocation ? const [3, 4] : const [1, 2, 3, 4];
    final targets = [
      for (final key in candidates.take(5))
        () {
          final sheet = targetWorkbook.sheet(key.$1);
          final parts = <String>[];
          if (sheet != null) {
            for (final column in targetColumns) {
              final cell = sheet.cell(key.$2, column);
              if (cell == null) continue;
              final text = compactDetail(cell.value);
              if (text.isNotEmpty) parts.add(text);
            }
          }
          return 'row ${key.$2}: ${parts.join(' / ')}';
        }(),
    ].join(' | ');
    details += '; candidate target rows: $targets';
    if (candidates.length > 5) details += ' | +${candidates.length - 5} more';
  }
  if (route != null) {
    final matcherSheet = switch (route.$1) {
      'Signed By Worley CIV' => 'CIV',
      'Signed By Worley MEC' => 'MEC',
      final sheet => sheet,
    };
    final matchedContexts = SplayTreeMap<String, SplayTreeSet<int>>(
      compareCodePoints,
    );
    final unmatchedContexts = SplayTreeSet<String>(compareCodePoints);
    for (final context in sourceContexts) {
      final label = contextLabel(context);
      final rows = SplayTreeSet<int>();
      for (final key in candidates) {
        final sheet = targetWorkbook.sheet(key.$1);
        if (sheet == null) continue;
        if (rowMatchesContext(matcherSheet, sheet, key.$2, context, targetWorkbook)) {
          rows.add(key.$2);
        }
      }
      if (rows.isEmpty) {
        unmatchedContexts.add(label);
      } else {
        matchedContexts[label] = rows;
      }
    }
    if (matchedContexts.isNotEmpty) {
      final matches = [
        for (final entry in matchedContexts.entries.take(5))
          '${entry.key} -> row(s) ${entry.value.join(', ')}',
      ].join(' | ');
      details += '; matched source context: $matches';
    }
    if (unmatchedContexts.isNotEmpty) {
      final label = unmatchedContexts.length == 1
          ? 'unmatched source context'
          : 'unmatched source contexts';
      details += '; $label: ${unmatchedContexts.take(5).join(' | ')}';
      if (unmatchedContexts.length > 5) {
        details += ' | +${unmatchedContexts.length - 5} more';
      }
    }
  }
  return details;
}

/// Port of `compact_detail`: collapse whitespace and keep 120 code points.
String compactDetail(String value) {
  final collapsed = rustSplitWhitespace(value).join(' ');
  final runes = collapsed.runes.take(120);
  return String.fromCharCodes(runes);
}

/// Port of `itp_identity`.
(String, String)? itpIdentity(String value) {
  final normalized = normalizeCodeText(value);
  final match = _itpRe.firstMatch(normalized);
  if (match == null) return null;
  return (match.group(1)!, match.group(2)!);
}

/// Port of `item_clause`.
String? itemClause(String value) {
  final match = _itemRe.firstMatch(normalizeCodeText(value));
  return match?.group(1);
}

/// Port of `full_item_identity`.
(String, String)? fullItemIdentity(String value) {
  final match = _itemFullRe.firstMatch(normalizeCodeText(value));
  if (match == null) return null;
  return (match.group(1)!, match.group(2)!);
}

/// Port of `normalize_code_text`.
String normalizeCodeText(String value) {
  var text = nfc(value);
  for (final dash in const ['˗', '‐', '‑', '‒', '–', '—', '−']) {
    text = text.replaceAll(dash, '-');
  }
  return rustUpper(rustTrim(text));
}

// --- primitive normalisation -----------------------------------------------

/// Port of `normalize_status`: NFC then Unicode whitespace trim.
String normalizeStatus(String value) => rustTrim(nfc(value));

/// Port of `normalize_header`: status-normalised, whitespace-collapsed, ASCII upper.
String normalizeHeader(String value) =>
    asciiUpper(rustSplitWhitespace(normalizeStatus(value)).join(' '));

/// Port of `str::trim` for Unicode White_Space.
String rustTrim(String value) {
  final runes = value.runes.toList();
  var start = 0;
  var end = runes.length;
  while (start < end && isRustWhitespaceRune(runes[start])) {
    start++;
  }
  while (end > start && isRustWhitespaceRune(runes[end - 1])) {
    end--;
  }
  if (start == 0 && end == runes.length) return value;
  return String.fromCharCodes(runes.sublist(start, end));
}

/// Port of `str::split_whitespace` collapsed as a list of tokens.
List<String> rustSplitWhitespace(String value) {
  final tokens = <String>[];
  final buffer = StringBuffer();
  for (final rune in value.runes) {
    if (isRustWhitespaceRune(rune)) {
      if (buffer.isNotEmpty) tokens.add(buffer.toString());
      buffer.clear();
    } else {
      buffer.writeCharCode(rune);
    }
  }
  if (buffer.isNotEmpty) tokens.add(buffer.toString());
  return tokens;
}

/// Unicode White_Space property (as used by Rust `char::is_whitespace`).
bool isRustWhitespaceRune(int rune) =>
    (rune >= 0x09 && rune <= 0x0D) ||
    rune == 0x20 ||
    rune == 0x85 ||
    rune == 0xA0 ||
    rune == 0x1680 ||
    (rune >= 0x2000 && rune <= 0x200A) ||
    rune == 0x2028 ||
    rune == 0x2029 ||
    rune == 0x202F ||
    rune == 0x205F ||
    rune == 0x3000;

/// Port of `to_uppercase` (full Unicode mapping).
String rustUpper(String value) => value.toUpperCase();

/// Port of `to_ascii_uppercase`.
String asciiUpper(String value) {
  final out = StringBuffer();
  for (final rune in value.runes) {
    if (rune >= 0x61 && rune <= 0x7A) {
      out.writeCharCode(rune - 0x20);
    } else {
      out.writeCharCode(rune);
    }
  }
  return out.toString();
}

/// ASCII-only case-insensitive equality (`eq_ignore_ascii_case`).
bool eqIgnoreAsciiCase(String a, String b) => asciiUpper(a) == asciiUpper(b);

/// Port of `parse::<u32>()`: optional leading '+', decimal digits, u32 range.
int? parseU32(String text) {
  final digits = text.startsWith('+') ? text.substring(1) : text;
  if (digits.isEmpty || !_digitsOnly.hasMatch(digits) || digits.length > 10) {
    return null;
  }
  final value = int.parse(digits);
  return value <= 0xFFFFFFFF ? value : null;
}

/// Unicode NFC. UNVERIFIED subset: composes the Latin/Turkish/Azeri and
/// Latin-1 precomposed letters that Follow Up inputs realistically contain.
/// Full canonical composition requires the Unicode data tables.
String nfc(String value) {
  var ascii = true;
  for (final unit in value.codeUnits) {
    if (unit >= 0x80) {
      ascii = false;
      break;
    }
  }
  if (ascii) return value;
  final out = StringBuffer();
  final runes = value.runes.toList();
  for (var i = 0; i < runes.length; i++) {
    final rune = runes[i];
    if (i + 1 < runes.length) {
      final composed = _composeTable[(rune, runes[i + 1])];
      if (composed != null) {
        out.writeCharCode(composed);
        i++;
        continue;
      }
    }
    out.writeCharCode(rune);
  }
  return out.toString();
}

final Map<(int, int), int> _composeTable = {
  for (final entry in _decompositionTable.entries)
    (entry.value.$1, entry.value.$2): entry.key,
};

/// Precomposed code point -> (base, combining mark) for the subset above.
const Map<int, (int, int)> _decompositionTable = {
  0x00C0: (0x41, 0x0300), 0x00C1: (0x41, 0x0301), 0x00C2: (0x41, 0x0302),
  0x00C3: (0x41, 0x0303), 0x00C4: (0x41, 0x0308), 0x00C5: (0x41, 0x030A),
  0x00C7: (0x43, 0x0327), 0x00C8: (0x45, 0x0300), 0x00C9: (0x45, 0x0301),
  0x00CA: (0x45, 0x0302), 0x00CB: (0x45, 0x0308), 0x00CC: (0x49, 0x0300),
  0x00CD: (0x49, 0x0301), 0x00CE: (0x49, 0x0302), 0x00CF: (0x49, 0x0308),
  0x00D1: (0x4E, 0x0303), 0x00D2: (0x4F, 0x0300), 0x00D3: (0x4F, 0x0301),
  0x00D4: (0x4F, 0x0302), 0x00D5: (0x4F, 0x0303), 0x00D6: (0x4F, 0x0308),
  0x00D9: (0x55, 0x0300), 0x00DA: (0x55, 0x0301), 0x00DB: (0x55, 0x0302),
  0x00DC: (0x55, 0x0308), 0x00DD: (0x59, 0x0301),
  0x00E0: (0x61, 0x0300), 0x00E1: (0x61, 0x0301), 0x00E2: (0x61, 0x0302),
  0x00E3: (0x61, 0x0303), 0x00E4: (0x61, 0x0308), 0x00E5: (0x61, 0x030A),
  0x00E7: (0x63, 0x0327), 0x00E8: (0x65, 0x0300), 0x00E9: (0x65, 0x0301),
  0x00EA: (0x65, 0x0302), 0x00EB: (0x65, 0x0308), 0x00EC: (0x69, 0x0300),
  0x00ED: (0x69, 0x0301), 0x00EE: (0x69, 0x0302), 0x00EF: (0x69, 0x0308),
  0x00F1: (0x6E, 0x0303), 0x00F2: (0x6F, 0x0300), 0x00F3: (0x6F, 0x0301),
  0x00F4: (0x6F, 0x0302), 0x00F5: (0x6F, 0x0303), 0x00F6: (0x6F, 0x0308),
  0x00F9: (0x75, 0x0300), 0x00FA: (0x75, 0x0301), 0x00FB: (0x75, 0x0302),
  0x00FC: (0x75, 0x0308), 0x00FD: (0x79, 0x0301), 0x00FF: (0x79, 0x0308),
  0x011E: (0x47, 0x0306), 0x011F: (0x67, 0x0306), 0x0130: (0x49, 0x0307),
  0x015E: (0x53, 0x0327), 0x015F: (0x73, 0x0327),
  0x0419: (0x0418, 0x0306), 0x0439: (0x0438, 0x0306),
};

/// Compact code-point comparison (matches UTF-8 byte order).
int compareCodePoints(String a, String b) {
  final ia = a.runes.iterator;
  final ib = b.runes.iterator;
  while (true) {
    final hasA = ia.moveNext();
    final hasB = ib.moveNext();
    if (!hasA) return hasB ? -1 : 0;
    if (!hasB) return 1;
    if (ia.current != ib.current) return ia.current < ib.current ? -1 : 1;
  }
}

int _compareSheetCol((String, int) a, (String, int) b) {
  final byName = compareCodePoints(a.$1, b.$1);
  return byName != 0 ? byName : a.$2.compareTo(b.$2);
}

int compareSlotKey(SlotKey a, SlotKey b) {
  final bySheet = compareCodePoints(a.$1, b.$1);
  if (bySheet != 0) return bySheet;
  if (a.$2 != b.$2) return a.$2.compareTo(b.$2);
  if (a.$3 != b.$3) return a.$3.compareTo(b.$3);
  return a.$4.compareTo(b.$4);
}

int compareRowColumn((int, int) a, (int, int) b) {
  if (a.$1 != b.$1) return a.$1.compareTo(b.$1);
  return a.$2.compareTo(b.$2);
}

/// Rust `str::lines()`: split on '\n' and strip one trailing '\r' per line.
Iterable<String> _rustLines(String value) sync* {
  if (value.isEmpty) return;
  final parts = value.split('\n');
  for (var i = 0; i < parts.length; i++) {
    if (i == parts.length - 1 && parts[i].isEmpty) break;
    var line = parts[i];
    if (line.endsWith('\r')) line = line.substring(0, line.length - 1);
    yield line;
  }
}

int _runeCount(String value) => value.runes.length;

/// Stable sort (Rust `sort_by` is stable; Dart `List.sort` is not).
List<T> stableSort<T>(List<T> items, int Function(T, T) compare) {
  final indexed = [for (var i = 0; i < items.length; i++) (i, items[i])];
  indexed.sort((a, b) {
    final c = compare(a.$2, b.$2);
    return c != 0 ? c : a.$1.compareTo(b.$1);
  });
  return [for (final pair in indexed) pair.$2];
}

T? _firstWhereOrNull<T>(Iterable<T> items, bool Function(T) test) {
  for (final item in items) {
    if (test(item)) return item;
  }
  return null;
}

bool _listEq<T>(List<T> a, List<T> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

bool _layoutListEquals(List<FollowUpLayout> a, List<FollowUpLayout> b) =>
    _listEq(a, b);

bool _jsonEquals(Object? a, Object? b) {
  if (identical(a, b)) return true;
  if (a is Map && b is Map) {
    if (a.length != b.length) return false;
    for (final key in a.keys) {
      if (!b.containsKey(key) || !_jsonEquals(a[key], b[key])) return false;
    }
    return true;
  }
  if (a is List && b is List) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (!_jsonEquals(a[i], b[i])) return false;
    }
    return true;
  }
  return a == b;
}

bool _rowsEqualExceptEnabled(FollowUpRow left, FollowUpRow right) =>
    left.copyWith(enabled: false) == right.copyWith(enabled: false);

FollowUpParsedWorkbook _readParsed(FollowUpOoxml ooxml, Uint8List bytes) {
  try {
    return ooxml.readWorkbookAndDiscovery(bytes);
  } on FollowUpOoxmlError catch (error) {
    throw mapFollowUpOoxmlError(error);
  }
}

bool _packageHasSignature(FollowUpOoxml ooxml, Uint8List bytes) {
  try {
    return ooxml.packageHasSignature(bytes);
  } on FollowUpOoxmlError catch (error) {
    throw mapFollowUpOoxmlError(error);
  }
}

String? _fileExtension(String path) {
  final dot = path.lastIndexOf('.');
  final sep = max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  if (dot < 0 || dot < sep) return null;
  return path.substring(dot + 1);
}

String _joinSibling(String path, String name) {
  final sep = max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  if (sep < 0) return name;
  return '${path.substring(0, sep + 1)}$name';
}

final Random _secureRandom = Random.secure();

String _randomHex32() {
  final out = StringBuffer();
  for (var i = 0; i < 32; i++) {
    out.write(_secureRandom.nextInt(16).toRadixString(16));
  }
  return out.toString();
}

/// Lowercase hex SHA-256 of [bytes] (pure Dart; no extra pub dependency).
String sha256Hex(Uint8List bytes) {
  final digest = _sha256(bytes);
  final out = StringBuffer();
  for (final byte in digest) {
    out.write(byte.toRadixString(16).padLeft(2, '0'));
  }
  return out.toString();
}

const List<int> _sha256K = [
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1,
  0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
  0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
  0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
  0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
  0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
  0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
  0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
  0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

Uint8List _sha256(Uint8List message) {
  final h = <int>[
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
  ];
  final bitLength = message.length * 8;
  final paddedLength = ((message.length + 9 + 63) ~/ 64) * 64;
  final padded = Uint8List(paddedLength)..setAll(0, message);
  padded[message.length] = 0x80;
  final lengthView = ByteData.view(padded.buffer);
  lengthView.setUint32(paddedLength - 8, bitLength ~/ 0x100000000, Endian.big);
  lengthView.setUint32(paddedLength - 4, bitLength & 0xFFFFFFFF, Endian.big);

  final w = List<int>.filled(64, 0);
  for (var offset = 0; offset < paddedLength; offset += 64) {
    for (var i = 0; i < 16; i++) {
      w[i] = lengthView.getUint32(offset + i * 4, Endian.big);
    }
    for (var i = 16; i < 64; i++) {
      final s0 = _rotr(w[i - 15], 7) ^ _rotr(w[i - 15], 18) ^ (w[i - 15] >> 3);
      final s1 = _rotr(w[i - 2], 17) ^ _rotr(w[i - 2], 19) ^ (w[i - 2] >> 10);
      w[i] = (w[i - 16] + s0 + w[i - 7] + s1) & 0xFFFFFFFF;
    }
    var a = h[0], b = h[1], c = h[2], d = h[3];
    var e = h[4], f = h[5], g = h[6], hh = h[7];
    for (var i = 0; i < 64; i++) {
      final s1 = _rotr(e, 6) ^ _rotr(e, 11) ^ _rotr(e, 25);
      final ch = (e & f) ^ ((~e) & 0xFFFFFFFF & g);
      final temp1 = (hh + s1 + ch + _sha256K[i] + w[i]) & 0xFFFFFFFF;
      final s0 = _rotr(a, 2) ^ _rotr(a, 13) ^ _rotr(a, 22);
      final maj = (a & b) ^ (a & c) ^ (b & c);
      final temp2 = (s0 + maj) & 0xFFFFFFFF;
      hh = g;
      g = f;
      f = e;
      e = (d + temp1) & 0xFFFFFFFF;
      d = c;
      c = b;
      b = a;
      a = (temp1 + temp2) & 0xFFFFFFFF;
    }
    h[0] = (h[0] + a) & 0xFFFFFFFF;
    h[1] = (h[1] + b) & 0xFFFFFFFF;
    h[2] = (h[2] + c) & 0xFFFFFFFF;
    h[3] = (h[3] + d) & 0xFFFFFFFF;
    h[4] = (h[4] + e) & 0xFFFFFFFF;
    h[5] = (h[5] + f) & 0xFFFFFFFF;
    h[6] = (h[6] + g) & 0xFFFFFFFF;
    h[7] = (h[7] + hh) & 0xFFFFFFFF;
  }
  final out = Uint8List(32);
  final outView = ByteData.view(out.buffer);
  for (var i = 0; i < 8; i++) {
    outView.setUint32(i * 4, h[i], Endian.big);
  }
  return out;
}

int _rotr(int value, int bits) =>
    ((value >> bits) | (value << (32 - bits))) & 0xFFFFFFFF;

/// Port of `CountingLimitWriter`: streams JSON and aborts past the budget.
class FollowUpByteBudget {
  FollowUpByteBudget(this.limit);
  final int limit;
  int count = 0;

  void write(List<int> bytes) {
    final next = count + bytes.length;
    if (next > limit) throw StateError('projection byte limit exceeded');
    count = next;
  }
}

void _validateJsonBudget(Object? value) {
  final bytes = utf8.encode(jsonEncode(value));
  if (bytes.length > _maxPreviewAuthorityBytes) throw limitExceededError();
}

/// Port of `validate_projection_budget`.
void validateProjectionBudget(Object? projection) =>
    _validateJsonBudget(projection);

// --- error constructors (messages match the Rust constants) ----------------

BackendError _workbookInvalidError() =>
    BackendError.validation('error.rfi.followUp.workbookInvalid');

BackendError _protectedError() =>
    BackendError.validation('error.rfi.followUp.protected');

BackendError _signedPackageError() =>
    BackendError.validation('error.rfi.followUp.signedPackage');

BackendError limitExceededError() =>
    BackendError.validation('error.rfi.followUp.limitExceeded');

BackendError _limitExceededError() => limitExceededError();

// --- strict JSON decoding ------------------------------------------------

JsonObject _strictObject(
  Object? value, {
  required Set<String> allowed,
  required Set<String> required,
}) {
  if (value is! Map) throw _workbookInvalidError();
  final map = Map<String, Object?>.from(value);
  for (final key in map.keys) {
    if (!allowed.contains(key)) throw _workbookInvalidError();
  }
  for (final key in required) {
    if (map[key] == null) throw _workbookInvalidError();
  }
  return map;
}

String _str(JsonObject map, String key) {
  final value = map[key];
  if (value is! String) throw _workbookInvalidError();
  return value;
}

String? _optStr(JsonObject map, String key) {
  final value = map[key];
  if (value == null) return null;
  if (value is! String) throw _workbookInvalidError();
  return value;
}

int _int(JsonObject map, String key) {
  final value = map[key];
  if (value is! int) throw _workbookInvalidError();
  return value;
}

bool _bool(JsonObject map, String key) {
  final value = map[key];
  if (value is! bool) throw _workbookInvalidError();
  return value;
}

List<String> _strList(JsonObject map, String key) {
  final value = map[key];
  if (value is! List) throw _workbookInvalidError();
  return [for (final item in value) item is String ? item : throw _workbookInvalidError()];
}

List<Object?> _list(JsonObject map, String key) {
  final value = map[key];
  if (value is! List) throw _workbookInvalidError();
  return value.cast<Object?>();
}

List<Object?> _listOrEmpty(JsonObject map, String key) {
  final value = map[key];
  if (value == null) return const [];
  if (value is! List) throw _workbookInvalidError();
  return value.cast<Object?>();
}

Object? _requiredField(JsonObject map, String key) {
  if (!map.containsKey(key) || map[key] == null) throw _workbookInvalidError();
  return map[key];
}

List<Object?> _requiredList(JsonObject map, String key) {
  final value = _requiredField(map, key);
  if (value is! List) throw _workbookInvalidError();
  return value.cast<Object?>();
}

List<Object?> _requiredEditorRows(JsonObject meta) {
  final editor = meta['editor'];
  if (editor is! Map) throw _workbookInvalidError();
  final rows = editor['rows'];
  if (rows is! List) throw _workbookInvalidError();
  return rows.cast<Object?>();
}

JsonObject _asJsonObject(Object? value) {
  if (value is! Map) throw _workbookInvalidError();
  return Map<String, Object?>.from(value);
}

T _decodeOrInvalid<T>(T Function() decode) {
  try {
    return decode();
  } on BackendError {
    throw _workbookInvalidError();
  } catch (_) {
    throw _workbookInvalidError();
  }
}
