import 'json.dart';
import 'path_selection.dart';

/// Latest print queue summary (`PrintQueueSummary`).
final class PrintQueueSummary {
  const PrintQueueSummary({
    required this.queueId,
    required this.status,
    required this.createdAt,
    required this.updatedAt,
    required this.counts,
    required this.resumable,
    required this.fileCount,
  });

  factory PrintQueueSummary.fromJson(JsonMap json) => PrintQueueSummary(
    queueId: jsonString(json, 'queueId'),
    status: jsonString(json, 'status'),
    createdAt: jsonString(json, 'createdAt'),
    updatedAt: jsonString(json, 'updatedAt'),
    counts: jsonIntMap(json, 'counts'),
    resumable: jsonInt(json, 'resumable'),
    fileCount: jsonInt(json, 'fileCount'),
  );

  final String queueId;
  final String status;
  final String createdAt;
  final String updatedAt;
  final Map<String, int> counts;
  final int resumable;
  final int fileCount;

  JsonMap toJson() => {
    'queueId': queueId,
    'status': status,
    'createdAt': createdAt,
    'updatedAt': updatedAt,
    'counts': counts,
    'resumable': resumable,
    'fileCount': fileCount,
  };
}

/// Document kinds an RFI print lookup can return (`documentType`).
enum RfiDocumentType {
  firstPage,
  sketch;

  static RfiDocumentType fromWire(String value) =>
      RfiDocumentType.values.firstWhere(
        (type) => type.name == value,
        orElse: () => throw FormatException('Unknown documentType "$value"'),
      );
}

/// One file matched by `rfi/printFind` (`RfiPrintCandidate`).
final class RfiPrintCandidate {
  const RfiPrintCandidate({
    required this.requested,
    required this.rfiCode,
    required this.documentType,
    required this.displayPath,
    required this.selection,
    required this.selected,
  });

  factory RfiPrintCandidate.fromJson(JsonMap json) => RfiPrintCandidate(
    requested: jsonString(json, 'requested'),
    rfiCode: jsonString(json, 'rfiCode'),
    documentType: RfiDocumentType.fromWire(jsonString(json, 'documentType')),
    displayPath: jsonString(json, 'displayPath'),
    selection: PathSelection.fromJson(jsonObject(json, 'selection')),
    selected: jsonBool(json, 'selected'),
  );

  final String requested;
  final String rfiCode;
  final RfiDocumentType documentType;
  final String displayPath;
  final PathSelection selection;
  final bool selected;

  JsonMap toJson() => {
    'requested': requested,
    'rfiCode': rfiCode,
    'documentType': documentType.name,
    'displayPath': displayPath,
    'selection': selection.toJson(),
    'selected': selected,
  };
}

/// Resolution of one requested RFI number (`RfiPrintResolution`).
final class RfiPrintResolution {
  const RfiPrintResolution({
    required this.requested,
    required this.rfiCodes,
    required this.missingFirstPage,
    required this.missingSketch,
    required this.ambiguous,
  });

  factory RfiPrintResolution.fromJson(JsonMap json) => RfiPrintResolution(
    requested: jsonString(json, 'requested'),
    rfiCodes: jsonStrings(json, 'rfiCodes'),
    missingFirstPage: jsonBool(json, 'missingFirstPage'),
    missingSketch: jsonBool(json, 'missingSketch'),
    ambiguous: jsonBool(json, 'ambiguous'),
  );

  final String requested;
  final List<String> rfiCodes;
  final bool missingFirstPage;
  final bool missingSketch;
  final bool ambiguous;

  JsonMap toJson() => {
    'requested': requested,
    'rfiCodes': rfiCodes,
    'missingFirstPage': missingFirstPage,
    'missingSketch': missingSketch,
    'ambiguous': ambiguous,
  };
}

/// Result of `rfi/printFind` (`RfiPrintFindResult`).
final class RfiPrintFindResult {
  const RfiPrintFindResult({
    required this.candidates,
    required this.resolutions,
    required this.visited,
    required this.truncated,
  });

  factory RfiPrintFindResult.fromJson(JsonMap json) => RfiPrintFindResult(
    candidates: jsonObjects(json, 'candidates', RfiPrintCandidate.fromJson),
    resolutions: jsonObjects(json, 'resolutions', RfiPrintResolution.fromJson),
    visited: jsonInt(json, 'visited'),
    truncated: jsonBool(json, 'truncated'),
  );

  final List<RfiPrintCandidate> candidates;
  final List<RfiPrintResolution> resolutions;
  final int visited;
  final bool truncated;

  JsonMap toJson() => {
    'candidates': [for (final candidate in candidates) candidate.toJson()],
    'resolutions': [for (final resolution in resolutions) resolution.toJson()],
    'visited': visited,
    'truncated': truncated,
  };
}
