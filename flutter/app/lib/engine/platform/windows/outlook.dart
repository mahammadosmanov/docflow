// Port of src-tauri/src/platform/windows/outlook.rs.
//
// Outlook is driven through the same [ComDispatchBackend] as Excel, so the
// selection checks, identity binding, fingerprinting, bounded text handling
// and MSG archive validation are exercised by tests on every platform.
//
// UNVERIFIED: the COM calls run through the Win32 backend, which is not
// executed in this environment. Outlook's object model, DISPID behavior and
// SaveAs format 9 (Unicode .msg) must be verified against a real Outlook
// installation on Windows before any acceptance claim.
//
// Minimal interface note: the adapter contract types below mirror
// src-tauri/src/outlook_reply_draft.rs, which belongs to the E7 unit
// (lib/engine/outlook_reply_draft.dart) and is not yet ported. When that file
// lands, import it and delete this block; names and semantics must match.
library;

import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart' as crypto;

import '../excel_windows_contract.dart';
import 'excel.dart';

const int outlookMailItemClass = 43;
const int outlookPlainTextFormat = 1;
const int outlookHtmlFormat = 2;
const int maxComTextChars = 1000000;
const int maxComIdChars = 4096;
const int outlookMsgUnicodeFormat = 9;
const int maxArchivedMsgBytes = 100 * 1024 * 1024;
const List<int> compoundFileHeader = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

// ---- Adapter contract (see note above) ------------------------------------

/// `OutlookFailureKind`.
enum OutlookFailureKind { unsupported, noEffect, outcomeUnknown }

/// `OutlookFailure`: a localization key with its effect classification.
class OutlookFailure implements Exception {
  const OutlookFailure(this.kind, this.key);

  const OutlookFailure.unsupported(String key)
    : this(OutlookFailureKind.unsupported, key);

  const OutlookFailure.noEffect(String key)
    : this(OutlookFailureKind.noEffect, key);

  const OutlookFailure.outcomeUnknown(String key)
    : this(OutlookFailureKind.outcomeUnknown, key);

  final OutlookFailureKind kind;
  final String key;

  @override
  String toString() => 'OutlookFailure($kind, $key)';
}

/// `MailItemIdentity`.
class MailItemIdentity {
  const MailItemIdentity({required this.entryId, required this.storeId});

  final String entryId;
  final String storeId;

  @override
  bool operator ==(Object other) =>
      other is MailItemIdentity &&
      other.entryId == entryId &&
      other.storeId == storeId;

  @override
  int get hashCode => Object.hash(entryId, storeId);
}

/// `SelectedMailItem`.
class SelectedMailItem {
  const SelectedMailItem({
    required this.identity,
    required this.subject,
    required this.fingerprint,
  });

  final MailItemIdentity identity;
  final String subject;
  final String fingerprint;
}

/// `ReplyContent`.
class ReplyContent {
  const ReplyContent({required this.plainText, required this.html});

  final String plainText;
  final String html;
}

/// `DraftIdentity`.
class DraftIdentity {
  const DraftIdentity({
    required this.entryId,
    required this.storeId,
    required this.fingerprint,
  });

  final String entryId;
  final String storeId;
  final String fingerprint;

  @override
  bool operator ==(Object other) =>
      other is DraftIdentity &&
      other.entryId == entryId &&
      other.storeId == storeId &&
      other.fingerprint == fingerprint;

  @override
  int get hashCode => Object.hash(entryId, storeId, fingerprint);
}

/// `OutlookDraft`. [dispose] releases the COM apartment and references that
/// Rust held through `Drop`.
abstract interface class OutlookDraft {
  void prependBody(ReplyContent content);
  void display();
  DraftIdentity? durableIdentity();
  void dispose();
}

/// `OutlookAdapter`.
abstract interface class OutlookAdapter {
  SelectedMailItem selectedMailItem();

  /// Re-resolves the active selection, verifies [expected], then calls ReplyAll.
  OutlookDraft replyAll(MailItemIdentity expected);

  /// Deletes only the exact durable draft when its fingerprint still matches.
  void deleteUnchangedDraft(DraftIdentity draft);

  Uint8List exportSelectedMsg(
    MailItemIdentity expected,
    String expectedFingerprint,
  );
}

// ---- Windows implementation -----------------------------------------------

/// `WindowsOutlookAdapter`.
final class WindowsOutlookAdapter implements OutlookAdapter {
  WindowsOutlookAdapter({required this.backendFactory, this.tempRoot});

  final ComDispatchBackend Function() backendFactory;

  /// Parent for the private MSG export workspace (system temp when null).
  final Directory? tempRoot;

  @override
  SelectedMailItem selectedMailItem() {
    final selected = _selectedMailSession(backendFactory);
    try {
      return SelectedMailItem(
        identity: selected.identity,
        subject: selected.subject,
        fingerprint: _draftFingerprint(selected.backend, selected.mail),
      );
    } finally {
      selected.close();
    }
  }

  @override
  OutlookDraft replyAll(MailItemIdentity expected) {
    final selected = _selectedMailSession(backendFactory);
    if (selected.identity != expected) {
      selected.close();
      throw const OutlookFailure.noEffect(
        'error.rfi.outlookDraft.selectionChanged',
      );
    }
    final ComDispatch reply;
    try {
      reply = _variantDispatch(
        _dispatchMethod(selected.backend, selected.mail, 'ReplyAll', const []),
      );
    } on Object {
      selected.close();
      throw const OutlookFailure.outcomeUnknown(
        'error.rfi.outlookDraft.replyAllFailed',
      );
    }
    // The apartment moves into the draft; the other references are released.
    selected.releaseReferencesOnly();
    return _WindowsReplyDraft(
      backend: selected.backend,
      apartment: selected.apartment,
      reply: reply,
    );
  }

  @override
  void deleteUnchangedDraft(DraftIdentity draft) {
    final backend = backendFactory();
    final apartment = _ComApartment.initialize(backend);
    try {
      final application = _activeOutlookApplication(backend);
      final namespace = _propertyDispatch(backend, application, 'Session');
      final ComDispatch item;
      try {
        item = _variantDispatch(
          _dispatchMethod(backend, namespace, 'GetItemFromID', [
            draft.entryId,
            draft.storeId,
          ]),
        );
      } on Object {
        throw const OutlookFailure.noEffect(
          'error.rfi.outlookDraft.draftMissingOrChanged',
        );
      }
      _ensureMailItem(backend, item);
      final actual = _mailIdentity(backend, item);
      if (actual.entryId != draft.entryId || actual.storeId != draft.storeId) {
        throw const OutlookFailure.noEffect(
          'error.rfi.outlookDraft.draftMissingOrChanged',
        );
      }
      if (!_propertyBool(backend, item, 'Saved')) {
        throw const OutlookFailure.noEffect(
          'error.rfi.outlookDraft.draftMissingOrChanged',
        );
      }
      if (_draftFingerprint(backend, item) != draft.fingerprint) {
        throw const OutlookFailure.noEffect(
          'error.rfi.outlookDraft.draftMissingOrChanged',
        );
      }
      try {
        _dispatchMethod(backend, item, 'Delete', const []);
      } on Object {
        throw const OutlookFailure.outcomeUnknown(
          'error.rfi.outlookDraft.deleteUnknown',
        );
      }
    } finally {
      apartment.dispose();
    }
  }

  @override
  Uint8List exportSelectedMsg(
    MailItemIdentity expected,
    String expectedFingerprint,
  ) {
    final selected = _selectedMailSession(backendFactory);
    try {
      if (selected.identity != expected ||
          _draftFingerprint(selected.backend, selected.mail) !=
              expectedFingerprint) {
        throw const OutlookFailure.noEffect(
          'error.rfi.outlookArchive.selectionChanged',
        );
      }
      final Directory temporary;
      try {
        temporary = (tempRoot ?? Directory.systemTemp).createTempSync(
          'docflow-outlook-',
        );
      } on Object {
        throw const OutlookFailure.noEffect(
          'error.rfi.outlookArchive.exportFailed',
        );
      }
      try {
        final path = '${temporary.path}${Platform.pathSeparator}message.msg';
        try {
          _dispatchMethod(selected.backend, selected.mail, 'SaveAs', [
            path,
            outlookMsgUnicodeFormat,
          ]);
        } on Object {
          throw const OutlookFailure.outcomeUnknown(
            'error.rfi.outlookArchive.exportFailed',
          );
        }
        final FileSystemEntityType type;
        final int length;
        try {
          type = FileSystemEntity.typeSync(path, followLinks: false);
          length = File(path).lengthSync();
        } on Object {
          throw const OutlookFailure.outcomeUnknown(
            'error.rfi.outlookArchive.exportFailed',
          );
        }
        if (type != FileSystemEntityType.file ||
            length < compoundFileHeader.length ||
            length > maxArchivedMsgBytes) {
          throw const OutlookFailure.noEffect(
            'error.rfi.outlookArchive.invalidMsg',
          );
        }
        final Uint8List bytes;
        try {
          bytes = File(path).readAsBytesSync();
        } on Object {
          throw const OutlookFailure.outcomeUnknown(
            'error.rfi.outlookArchive.exportFailed',
          );
        }
        if (!_startsWith(bytes, compoundFileHeader)) {
          throw const OutlookFailure.noEffect(
            'error.rfi.outlookArchive.invalidMsg',
          );
        }
        return bytes;
      } finally {
        temporary.deleteSync(recursive: true);
      }
    } finally {
      selected.close();
    }
  }
}

bool _startsWith(List<int> bytes, List<int> prefix) {
  if (bytes.length < prefix.length) {
    return false;
  }
  for (var index = 0; index < prefix.length; index++) {
    if (bytes[index] != prefix[index]) {
      return false;
    }
  }
  return true;
}

final class _WindowsReplyDraft implements OutlookDraft {
  _WindowsReplyDraft({
    required this.backend,
    required this.apartment,
    required this.reply,
  });

  final ComDispatchBackend backend;
  final _ComApartment apartment;
  final ComDispatch reply;

  @override
  void prependBody(ReplyContent content) {
    final int format;
    try {
      format = _propertyI32(backend, reply, 'BodyFormat');
    } on Object {
      throw const OutlookFailure.outcomeUnknown(
        'error.rfi.outlookDraft.bodyPrependFailed',
      );
    }
    switch (format) {
      case outlookHtmlFormat:
        final String existing;
        try {
          existing = _boundedPropertyString(backend, reply, 'HTMLBody');
        } on Object {
          throw const OutlookFailure.outcomeUnknown(
            'error.rfi.outlookDraft.bodyPrependFailed',
          );
        }
        final body = _boundedJoin(content.html, existing);
        try {
          _propertyPutString(backend, reply, 'HTMLBody', body);
        } on Object {
          throw const OutlookFailure.outcomeUnknown(
            'error.rfi.outlookDraft.bodyPrependFailed',
          );
        }
      case outlookPlainTextFormat:
        final String existing;
        try {
          existing = _boundedPropertyString(backend, reply, 'Body');
        } on Object {
          throw const OutlookFailure.outcomeUnknown(
            'error.rfi.outlookDraft.bodyPrependFailed',
          );
        }
        final prefix = '${content.plainText}\r\n\r\n';
        final body = _boundedJoin(prefix, existing);
        try {
          _propertyPutString(backend, reply, 'Body', body);
        } on Object {
          throw const OutlookFailure.outcomeUnknown(
            'error.rfi.outlookDraft.bodyPrependFailed',
          );
        }
      default:
        throw const OutlookFailure.outcomeUnknown(
          'error.rfi.outlookDraft.bodyFormatUnsupported',
        );
    }
  }

  @override
  void display() {
    try {
      _dispatchMethod(backend, reply, 'Display', const []);
    } on Object {
      throw const OutlookFailure.outcomeUnknown(
        'error.rfi.outlookDraft.displayUnknown',
      );
    }
  }

  @override
  DraftIdentity? durableIdentity() {
    try {
      if (!_propertyBool(backend, reply, 'Saved')) {
        return null;
      }
      final identity = _mailIdentity(backend, reply);
      final fingerprint = _draftFingerprint(backend, reply);
      return DraftIdentity(
        entryId: identity.entryId,
        storeId: identity.storeId,
        fingerprint: fingerprint,
      );
    } on Object {
      return null;
    }
  }

  @override
  void dispose() {
    try {
      reply.release();
    } finally {
      apartment.dispose();
    }
  }
}

final class _SelectedMailSession {
  _SelectedMailSession({
    required this.backend,
    required this.apartment,
    required this.references,
    required this.mail,
    required this.identity,
    required this.subject,
  });

  final ComDispatchBackend backend;
  final _ComApartment apartment;

  /// Application, explorer, selection, and mail references held for the call.
  final List<ComDispatch> references;
  final ComDispatch mail;
  final MailItemIdentity identity;
  final String subject;

  void releaseReferencesOnly() {
    for (final reference in references.reversed) {
      reference.release();
    }
    references.clear();
    mail.release();
  }

  void close() {
    releaseReferencesOnly();
    apartment.dispose();
  }
}

_SelectedMailSession _selectedMailSession(
  ComDispatchBackend Function() backendFactory,
) {
  final backend = backendFactory();
  final apartment = _ComApartment.initialize(backend);
  final references = <ComDispatch>[];
  try {
    final application = _activeOutlookApplication(backend);
    references.add(application);
    final ComDispatch explorer;
    try {
      explorer = _propertyDispatch(backend, application, 'ActiveExplorer');
    } on Object {
      throw const OutlookFailure.noEffect(
        'error.rfi.outlookDraft.selectionRequired',
      );
    }
    references.add(explorer);
    final selection = _propertyDispatch(backend, explorer, 'Selection');
    references.add(selection);
    if (_propertyI32(backend, selection, 'Count') != 1) {
      throw const OutlookFailure.noEffect(
        'error.rfi.outlookDraft.singleSelectionRequired',
      );
    }
    final ComDispatch mail;
    try {
      mail = _variantDispatch(
        _dispatchMethod(backend, selection, 'Item', [1]),
      );
    } on Object {
      throw const OutlookFailure.noEffect(
        'error.rfi.outlookDraft.selectionRequired',
      );
    }
    _ensureMailItem(backend, mail);
    final identity = _mailIdentity(backend, mail);
    final subject = _boundedPropertyString(backend, mail, 'Subject');
    return _SelectedMailSession(
      backend: backend,
      apartment: apartment,
      references: references,
      mail: mail,
      identity: identity,
      subject: subject,
    );
  } on Object {
    for (final reference in references.reversed) {
      reference.release();
    }
    apartment.dispose();
    rethrow;
  }
}

void _ensureMailItem(ComDispatchBackend backend, ComDispatch item) {
  if (_propertyI32(backend, item, 'Class') != outlookMailItemClass) {
    throw const OutlookFailure.noEffect('error.rfi.outlookDraft.mailItemRequired');
  }
}

MailItemIdentity _mailIdentity(ComDispatchBackend backend, ComDispatch mail) {
  final entryId = _boundedId(_propertyString(backend, mail, 'EntryID'));
  final parent = _propertyDispatch(backend, mail, 'Parent');
  final storeId = _boundedId(_propertyString(backend, parent, 'StoreID'));
  return MailItemIdentity(entryId: entryId, storeId: storeId);
}

/// `draft_fingerprint`: SHA-256 over length-prefixed text, size, last
/// modification time bits and the attachment count, all little-endian.
String _draftFingerprint(ComDispatchBackend backend, ComDispatch mail) {
  final builder = BytesBuilder(copy: false);
  for (final property in ['Subject', 'Body', 'HTMLBody', 'To', 'CC', 'BCC']) {
    final value = _boundedPropertyString(backend, mail, property);
    final bytes = utf8.encode(value);
    builder.add(_u64le(bytes.length));
    builder.add(bytes);
  }
  builder.add(_i32le(_propertyI32(backend, mail, 'Size')));
  final date = _propertyDate(backend, mail, 'LastModificationTime');
  builder.add(_f64BitsLe(date));
  final attachments = _propertyDispatch(backend, mail, 'Attachments');
  builder.add(_i32le(_propertyI32(backend, attachments, 'Count')));
  return crypto.sha256.convert(builder.takeBytes()).toString();
}

Uint8List _u64le(int value) {
  final data = ByteData(8)..setUint64(0, value, Endian.little);
  return data.buffer.asUint8List();
}

Uint8List _i32le(int value) {
  final data = ByteData(4)..setInt32(0, value, Endian.little);
  return data.buffer.asUint8List();
}

Uint8List _f64BitsLe(double value) {
  final data = ByteData(8)..setFloat64(0, value, Endian.little);
  return data.buffer.asUint8List();
}

String _boundedId(String value) {
  if (value.isNotEmpty &&
      value.runes.length <= maxComIdChars &&
      !value.runes.any(_isControl)) {
    return value;
  }
  throw const OutlookFailure.noEffect('error.rfi.outlookDraft.selectionInvalid');
}

/// `char::is_control`: Unicode general categories Cc (C0, DEL, C1).
bool _isControl(int rune) =>
    rune <= 0x1F || (rune >= 0x7F && rune <= 0x9F);

String _boundedPropertyString(
  ComDispatchBackend backend,
  ComDispatch dispatch,
  String name,
) {
  final value = _propertyString(backend, dispatch, name);
  if (value.runes.length <= maxComTextChars) {
    return value;
  }
  throw const OutlookFailure.noEffect('error.rfi.outlookDraft.contentTooLarge');
}

String _boundedJoin(String prefix, String existing) {
  if (prefix.runes.length + existing.runes.length > maxComTextChars) {
    throw const OutlookFailure.outcomeUnknown(
      'error.rfi.outlookDraft.contentTooLarge',
    );
  }
  return '$prefix$existing';
}

final class _ComApartment {
  _ComApartment._(this._backend);

  final ComDispatchBackend _backend;
  bool _active = true;

  static _ComApartment initialize(ComDispatchBackend backend) {
    try {
      backend.initializeSta();
    } on ComBackendFailure {
      throw const OutlookFailure.unsupported('error.rfi.outlookDraft.comUnavailable');
    }
    return _ComApartment._(backend);
  }

  void dispose() {
    if (_active) {
      _active = false;
      _backend.uninitialize();
    }
  }
}

ComDispatch _activeOutlookApplication(ComDispatchBackend backend) {
  try {
    return backend.getActiveObject('Outlook.Application');
  } on ComBackendFailure catch (failure) {
    if (failure.stage == 'resolve') {
      throw const OutlookFailure.unsupported('error.rfi.outlookDraft.outlookUnavailable');
    }
    throw const OutlookFailure.unsupported('error.rfi.outlookDraft.outlookNotRunning');
  }
}

ComDispatch _propertyDispatch(
  ComDispatchBackend backend,
  ComDispatch dispatch,
  String name,
) {
  try {
    return _variantDispatch(_property(backend, dispatch, name));
  } on Object {
    throw const OutlookFailure.noEffect('error.rfi.outlookDraft.comFailed');
  }
}

String _propertyString(
  ComDispatchBackend backend,
  ComDispatch dispatch,
  String name,
) {
  try {
    final value = _property(backend, dispatch, name);
    if (value is String) {
      return value;
    }
    throw const ComBackendFailure('result_decode', 0x80020005);
  } on Object {
    throw const OutlookFailure.noEffect('error.rfi.outlookDraft.comFailed');
  }
}

int _propertyI32(ComDispatchBackend backend, ComDispatch dispatch, String name) {
  try {
    final value = _property(backend, dispatch, name);
    if (value is int) {
      return value;
    }
    throw const ComBackendFailure('result_decode', 0x80020005);
  } on Object {
    throw const OutlookFailure.noEffect('error.rfi.outlookDraft.comFailed');
  }
}

bool _propertyBool(ComDispatchBackend backend, ComDispatch dispatch, String name) {
  try {
    final value = _property(backend, dispatch, name);
    if (value is bool) {
      return value;
    }
    throw const ComBackendFailure('result_decode', 0x80020005);
  } on Object {
    throw const OutlookFailure.noEffect('error.rfi.outlookDraft.comFailed');
  }
}

double _propertyDate(ComDispatchBackend backend, ComDispatch dispatch, String name) {
  try {
    final value = _property(backend, dispatch, name);
    if (value is double) {
      return value;
    }
    throw const ComBackendFailure('result_decode', 0x80020005);
  } on Object {
    throw const OutlookFailure.noEffect('error.rfi.outlookDraft.comFailed');
  }
}

ComResult _property(ComDispatchBackend backend, ComDispatch dispatch, String name) =>
    backend.invoke(dispatch, name, DispatchInvokeKind.propertyGet, const [], const []);

void _propertyPutString(
  ComDispatchBackend backend,
  ComDispatch dispatch,
  String name,
  String value,
) {
  backend.invoke(
    dispatch,
    name,
    DispatchInvokeKind.propertyPut,
    [EncodedString(value)],
    const [-3],
  );
}

ComResult _dispatchMethod(
  ComDispatchBackend backend,
  ComDispatch dispatch,
  String name,
  List<Object> arguments,
) {
  // Rust `invoke` reverses the argument list before building DISPPARAMS.
  final encoded = arguments.reversed.map<EncodedVariant>((value) {
    return switch (value) {
      String text => EncodedString(text),
      int number => EncodedI32(number),
      bool flag => EncodedBool(flag),
      _ => throw ArgumentError('unsupported argument ${value.runtimeType}'),
    };
  }).toList();
  return backend.invoke(
    dispatch,
    name,
    DispatchInvokeKind.method,
    encoded,
    const [],
  );
}

ComDispatch _variantDispatch(ComResult value) {
  if (value is ComDispatch) {
    return value;
  }
  throw const ComBackendFailure('result_decode', 0x80020005);
}
