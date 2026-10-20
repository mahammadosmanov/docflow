// Port of src-tauri/src/platform/excel_windows_contract.rs.
//
// Pure contract logic for the Excel COM export path: Mark-of-the-Web zone
// parsing, workbook pre-open safety, the exact COM operation plan, dispatch
// member whitelist and dispatch argument encoding. Nothing here calls Windows
// APIs, so the whole file is verified on every platform.
library;

import 'dart:convert';
import 'dart:typed_data';

import '../cap_fs.dart' show AnchoredPath;
import '../ooxml.dart' show WorkbookSafetyFacts, WorksheetVisibility;
import 'excel.dart' show WorkbookSourcePolicy;
import 'excel_print_ports.dart';

const int maxZoneIdentifierBytes = 16 * 1024;
const int maxDispatchStringChars = 32767;

/// `ZoneClassification`. Only the three zones the policy admits.
enum ZoneClassification { localMachine, localIntranet, trusted }

/// `ZoneIdentifierError`.
class ZoneIdentifierError implements Exception {
  const ZoneIdentifierError();
}

final RegExp _zoneIdPattern = RegExp(r'^\+?[0-9]+$');

/// `parse_zone_identifier`: fail-closed parser of the `Zone.Identifier`
/// alternate stream. Zones 0, 1 and 2 are accepted; 3 and above are blocked.
ZoneClassification parseZoneIdentifier(List<int> contents) {
  if (contents.isEmpty || contents.length > maxZoneIdentifierBytes) {
    throw const ZoneIdentifierError();
  }
  final String text;
  try {
    text = utf8.decode(contents);
  } on FormatException {
    throw const ZoneIdentifierError();
  }
  var sawSection = false;
  int? zoneId;
  for (final rawLine in _rustLines(text)) {
    final line = rawLine.trim();
    if (line.isEmpty || line.startsWith(';')) {
      continue;
    }
    if (line.startsWith('[')) {
      if (line != '[ZoneTransfer]' || sawSection) {
        throw const ZoneIdentifierError();
      }
      sawSection = true;
      continue;
    }
    if (!sawSection) {
      throw const ZoneIdentifierError();
    }
    final separator = line.indexOf('=');
    if (separator < 0) {
      throw const ZoneIdentifierError();
    }
    final key = line.substring(0, separator);
    final value = line.substring(separator + 1);
    if (key == 'ZoneId') {
      if (zoneId != null) {
        throw const ZoneIdentifierError();
      }
      zoneId = _parseU8(value);
    } else if (key.isEmpty) {
      throw const ZoneIdentifierError();
    }
  }
  if (!sawSection) {
    throw const ZoneIdentifierError();
  }
  final id = zoneId;
  if (id == null) {
    throw const ZoneIdentifierError();
  }
  return switch (id) {
    0 => ZoneClassification.localMachine,
    1 => ZoneClassification.localIntranet,
    2 => ZoneClassification.trusted,
    _ => throw const ZoneIdentifierError(),
  };
}

/// Rust `str::parse::<u8>()`: optional `+`, ASCII digits, value at most 255.
int _parseU8(String value) {
  if (!_zoneIdPattern.hasMatch(value)) {
    throw const ZoneIdentifierError();
  }
  final parsed = int.parse(value.startsWith('+') ? value.substring(1) : value);
  if (parsed > 255) {
    throw const ZoneIdentifierError();
  }
  return parsed;
}

/// Rust `str::lines`: split on `\n`, drop one trailing `\r` per line, and do
/// not yield a final empty segment after a trailing newline.
Iterable<String> _rustLines(String text) sync* {
  final parts = text.split('\n');
  for (var index = 0; index < parts.length; index++) {
    var line = parts[index];
    if (index == parts.length - 1 && line.isEmpty) {
      return;
    }
    if (line.endsWith('\r')) {
      line = line.substring(0, line.length - 1);
    }
    yield line;
  }
}

/// `WorkbookSafetyError`.
class WorkbookSafetyError implements Exception {
  const WorkbookSafetyError();
}

/// `validate_workbook_safety`: pre-open refusal. VBA projects are allowed;
/// XLM macro sheets, protection, external links, connections and query tables
/// are not.
void validateWorkbookSafety(WorkbookSafetyFacts safety) {
  if (safety.hasXlmMacroSheet ||
      safety.hasWorkbookProtection ||
      safety.externalLinkCount != 0 ||
      safety.connectionCount != 0 ||
      safety.queryTableCount != 0) {
    throw const WorkbookSafetyError();
  }
}

/// `ZoneIdentifierReader`: reads the `Zone.Identifier` stream of a source.
/// Returns null when the stream is absent. Throws on read failure.
abstract interface class ZoneIdentifierReader {
  Uint8List? readZoneIdentifier(AnchoredPath source);
}

/// `AnchoredWorkbookSourcePolicy`.
class AnchoredWorkbookSourcePolicy implements WorkbookSourcePolicy {
  const AnchoredWorkbookSourcePolicy(this.reader);

  final ZoneIdentifierReader reader;

  @override
  void validateSource(AnchoredPath source, WorkbookSafetyFacts safety) {
    try {
      validateWorkbookSafety(safety);
    } on WorkbookSafetyError {
      throw PrintFailure.unsupported('error.print.excelWorkbookUnsafe');
    }
    final Uint8List? zoneIdentifier;
    try {
      zoneIdentifier = reader.readZoneIdentifier(source);
    } on Object {
      throw PrintFailure.unsupported('error.print.excelSourceMotwBlocked');
    }
    if (zoneIdentifier != null) {
      try {
        parseZoneIdentifier(zoneIdentifier);
      } on ZoneIdentifierError {
        throw PrintFailure.unsupported('error.print.excelSourceMotwBlocked');
      }
    }
  }
}

/// `ComValue`: a dispatch argument before encoding.
sealed class ComValue {
  const ComValue();
}

class ComI32 extends ComValue {
  const ComI32(this.value);
  final int value;

  @override
  bool operator ==(Object other) => other is ComI32 && other.value == value;

  @override
  int get hashCode => value.hashCode;

  @override
  String toString() => 'ComI32($value)';
}

class ComBool extends ComValue {
  const ComBool(this.value);
  final bool value;

  @override
  bool operator ==(Object other) => other is ComBool && other.value == value;

  @override
  int get hashCode => value.hashCode + 7;

  @override
  String toString() => 'ComBool($value)';
}

class ComString extends ComValue {
  const ComString(this.value);
  final String value;

  @override
  bool operator ==(Object other) => other is ComString && other.value == value;

  @override
  int get hashCode => value.hashCode + 13;

  @override
  String toString() => 'ComString($value)';
}

class ComMissing extends ComValue {
  const ComMissing();

  @override
  bool operator ==(Object other) => other is ComMissing;

  @override
  int get hashCode => 19;

  @override
  String toString() => 'ComMissing';
}

/// `DispatchInvokeKind`.
enum DispatchInvokeKind { method, propertyGet, propertyPut }

/// `EncodedVariant`. `EncodedError` carries an HRESULT such as
/// `DISP_E_PARAMNOTFOUND`.
sealed class EncodedVariant {
  const EncodedVariant();
}

class EncodedI32 extends EncodedVariant {
  const EncodedI32(this.value);
  final int value;

  @override
  bool operator ==(Object other) => other is EncodedI32 && other.value == value;

  @override
  int get hashCode => value.hashCode;

  @override
  String toString() => 'EncodedI32($value)';
}

class EncodedBool extends EncodedVariant {
  const EncodedBool(this.value);
  final bool value;

  @override
  bool operator ==(Object other) =>
      other is EncodedBool && other.value == value;

  @override
  int get hashCode => value.hashCode + 7;

  @override
  String toString() => 'EncodedBool($value)';
}

class EncodedString extends EncodedVariant {
  const EncodedString(this.value);
  final String value;

  @override
  bool operator ==(Object other) =>
      other is EncodedString && other.value == value;

  @override
  int get hashCode => value.hashCode + 13;

  @override
  String toString() => 'EncodedString($value)';
}

class EncodedError extends EncodedVariant {
  const EncodedError(this.hresult);
  final int hresult;

  @override
  bool operator ==(Object other) =>
      other is EncodedError && other.hresult == hresult;

  @override
  int get hashCode => hresult.hashCode + 19;

  @override
  String toString() => 'EncodedError($hresult)';
}

/// `EncodedDispatchParameters`. `flags` are DISPATCH_* bits.
class EncodedDispatchParameters {
  const EncodedDispatchParameters({
    required this.flags,
    required this.arguments,
    required this.namedDispids,
  });

  final int flags;
  final List<EncodedVariant> arguments;
  final List<int> namedDispids;

  @override
  bool operator ==(Object other) =>
      other is EncodedDispatchParameters &&
      other.flags == flags &&
      _listEquals(other.arguments, arguments) &&
      _listEquals(other.namedDispids, namedDispids);

  @override
  int get hashCode => Object.hash(flags, arguments.length, namedDispids.length);
}

bool _listEquals<T>(List<T> left, List<T> right) {
  if (left.length != right.length) {
    return false;
  }
  for (var index = 0; index < left.length; index++) {
    if (left[index] != right[index]) {
      return false;
    }
  }
  return true;
}

/// `DISP_E_PARAMNOTFOUND` (0x80020004) as a signed 32-bit value.
const int dispEParamNotFound = -0x7FFDFFFC;

/// `encode_dispatch_parameters`.
EncodedDispatchParameters encodeDispatchParameters(
  List<ComValue> values,
  DispatchInvokeKind kind,
) {
  final arguments = values
      .map<EncodedVariant>(
        (value) => switch (value) {
          ComI32(:final value) => EncodedI32(value),
          ComBool(:final value) => EncodedBool(value),
          ComString(:final value) => EncodedString(value),
          ComMissing() => const EncodedError(dispEParamNotFound),
        },
      )
      .toList();
  final (flags, namedDispids) = switch (kind) {
    DispatchInvokeKind.method => (1, <int>[]),
    DispatchInvokeKind.propertyGet => (2, <int>[]),
    DispatchInvokeKind.propertyPut => (4, <int>[-3]),
  };
  return EncodedDispatchParameters(
    flags: flags,
    arguments: arguments,
    namedDispids: namedDispids,
  );
}

/// `ComObject`: objects released at teardown.
enum ComObject { application, workbooks, workbook, worksheets, worksheet }

/// `ComOperation`: one step of the Excel automation plan.
sealed class ComOperation {
  const ComOperation();
}

class ComInitializeSta extends ComOperation {
  const ComInitializeSta();
  @override
  bool operator ==(Object other) => other is ComInitializeSta;
  @override
  int get hashCode => 1;
  @override
  String toString() => 'InitializeSta';
}

class ComCreateApplication extends ComOperation {
  const ComCreateApplication();
  @override
  bool operator ==(Object other) => other is ComCreateApplication;
  @override
  int get hashCode => 2;
  @override
  String toString() => 'CreateApplication';
}

class ComSetProperty extends ComOperation {
  const ComSetProperty({
    required this.name,
    required this.value,
    this.namedPropertyPut = true,
  });

  final String name;
  final ComValue value;
  final bool namedPropertyPut;

  @override
  bool operator ==(Object other) =>
      other is ComSetProperty &&
      other.name == name &&
      other.value == value &&
      other.namedPropertyPut == namedPropertyPut;

  @override
  int get hashCode => Object.hash(name, value, namedPropertyPut);

  @override
  String toString() => 'SetProperty($name, $value)';
}

class ComOpenWorkbook extends ComOperation {
  const ComOpenWorkbook({required this.arguments});
  final List<ComValue> arguments;

  @override
  bool operator ==(Object other) =>
      other is ComOpenWorkbook && _listEquals(other.arguments, arguments);

  @override
  int get hashCode => arguments.length;
}

class ComResolveWorksheet extends ComOperation {
  const ComResolveWorksheet({required this.arguments});
  final List<ComValue> arguments;

  @override
  bool operator ==(Object other) =>
      other is ComResolveWorksheet && _listEquals(other.arguments, arguments);

  @override
  int get hashCode => arguments.length + 3;
}

class ComVerifyWorksheet extends ComOperation {
  const ComVerifyWorksheet({
    required this.expectedName,
    required this.expectedVisibility,
  });

  final String expectedName;
  final int expectedVisibility;

  @override
  bool operator ==(Object other) =>
      other is ComVerifyWorksheet &&
      other.expectedName == expectedName &&
      other.expectedVisibility == expectedVisibility;

  @override
  int get hashCode => Object.hash(expectedName, expectedVisibility);
}

class ComExportWorksheetPdf extends ComOperation {
  const ComExportWorksheetPdf({required this.arguments});
  final List<ComValue> arguments;

  @override
  bool operator ==(Object other) =>
      other is ComExportWorksheetPdf && _listEquals(other.arguments, arguments);

  @override
  int get hashCode => arguments.length + 5;
}

class ComCloseWorkbook extends ComOperation {
  const ComCloseWorkbook({required this.arguments});
  final List<ComValue> arguments;

  @override
  bool operator ==(Object other) =>
      other is ComCloseWorkbook && _listEquals(other.arguments, arguments);

  @override
  int get hashCode => arguments.length + 11;
}

class ComQuitApplication extends ComOperation {
  const ComQuitApplication();
  @override
  bool operator ==(Object other) => other is ComQuitApplication;
  @override
  int get hashCode => 6;
  @override
  String toString() => 'QuitApplication';
}

class ComReleaseObject extends ComOperation {
  const ComReleaseObject(this.object);
  final ComObject object;

  @override
  bool operator ==(Object other) =>
      other is ComReleaseObject && other.object == object;

  @override
  int get hashCode => object.hashCode + 17;
  @override
  String toString() => 'ReleaseObject($object)';
}

class ComUninitializeSta extends ComOperation {
  const ComUninitializeSta();
  @override
  bool operator ==(Object other) => other is ComUninitializeSta;
  @override
  int get hashCode => 8;
  @override
  String toString() => 'UninitializeSta';
}

/// `ComPlanRequest`.
class ComPlanRequest {
  const ComPlanRequest({
    required this.workbookPath,
    required this.pdfPath,
    required this.workbookIndex,
    required this.worksheetName,
    required this.worksheetVisibility,
  });

  final String workbookPath;
  final String pdfPath;
  final int workbookIndex;
  final String worksheetName;
  final WorksheetVisibility worksheetVisibility;
}

/// `ComPlanError`.
class ComPlanError implements Exception {
  const ComPlanError();
}

/// `validate_dispatch_member`: the only members the COM layer may invoke.
void validateDispatchMember(String name) {
  switch (name) {
    case 'AutomationSecurity':
    case 'EnableEvents':
    case 'DisplayAlerts':
    case 'AskToUpdateLinks':
    case 'Visible':
    case 'Workbooks':
    case 'Open':
    case 'Worksheets':
    case 'Item':
    case 'Name':
    case 'ExportAsFixedFormat':
    case 'Close':
    case 'Quit':
      return;
    default:
      throw const ComPlanError();
  }
}

/// `build_com_operation_plan`: the governing lockdown order (security and
/// alert suppression before Open), then export, close, quit and release.
List<ComOperation> buildComOperationPlan(ComPlanRequest request) {
  if (request.workbookIndex == 0 ||
      request.workbookIndex > 250 ||
      !_isBoundedDispatchString(request.workbookPath) ||
      !_isBoundedDispatchString(request.pdfPath) ||
      !_isBoundedDispatchString(request.worksheetName)) {
    throw const ComPlanError();
  }
  return [
    const ComInitializeSta(),
    const ComCreateApplication(),
    _setProperty('AutomationSecurity', const ComI32(3)),
    _setProperty('EnableEvents', const ComBool(false)),
    _setProperty('DisplayAlerts', const ComBool(false)),
    _setProperty('AskToUpdateLinks', const ComBool(false)),
    _setProperty('Visible', const ComBool(false)),
    ComOpenWorkbook(
      arguments: [
        const ComI32(0),
        const ComMissing(),
        const ComBool(false),
        const ComMissing(),
        const ComBool(false),
        const ComMissing(),
        const ComMissing(),
        const ComMissing(),
        const ComBool(true),
        const ComString(''),
        const ComString(''),
        const ComMissing(),
        const ComBool(true),
        const ComI32(0),
        ComString(request.workbookPath),
      ],
    ),
    ComResolveWorksheet(arguments: [ComI32(request.workbookIndex)]),
    ComVerifyWorksheet(
      expectedName: request.worksheetName,
      expectedVisibility: _worksheetVisibilityValue(
        request.worksheetVisibility,
      ),
    ),
    ComExportWorksheetPdf(
      arguments: [
        const ComMissing(),
        const ComBool(false),
        const ComMissing(),
        const ComMissing(),
        const ComBool(false),
        const ComBool(true),
        const ComI32(0),
        ComString(request.pdfPath),
        const ComI32(0),
      ],
    ),
    ComCloseWorkbook(arguments: [const ComBool(false)]),
    const ComQuitApplication(),
    const ComReleaseObject(ComObject.worksheet),
    const ComReleaseObject(ComObject.worksheets),
    const ComReleaseObject(ComObject.workbook),
    const ComReleaseObject(ComObject.workbooks),
    const ComReleaseObject(ComObject.application),
    const ComUninitializeSta(),
  ];
}

bool _isBoundedDispatchString(String value) =>
    value.isNotEmpty &&
    !value.contains('\u0000') &&
    value.codeUnits.length <= maxDispatchStringChars;

int _worksheetVisibilityValue(WorksheetVisibility visibility) =>
    switch (visibility) {
      WorksheetVisibility.visible => -1,
      WorksheetVisibility.hidden => 0,
      WorksheetVisibility.veryHidden => 2,
    };

ComOperation _setProperty(String name, ComValue value) =>
    ComSetProperty(name: name, value: value);
