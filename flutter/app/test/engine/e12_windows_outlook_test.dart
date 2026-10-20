// Verifies WindowsOutlookAdapter against a fake Outlook object graph. The
// Win32 backend that would talk to real Outlook is UNVERIFIED on Linux.
import 'dart:io';

import 'package:docflow/engine/platform/excel_windows_contract.dart';
import 'package:docflow/engine/platform/windows/excel.dart';
import 'package:docflow/engine/platform/windows/outlook.dart';
import 'package:flutter_test/flutter_test.dart';

/// Outlook object: named properties, methods, and optional child objects.
class _Obj implements ComDispatch {
  _Obj(this.name, this.backend);

  final String name;
  final _OutlookBackend backend;
  final Map<String, Object> props = {};
  bool released = false;

  @override
  void release() {
    released = true;
  }
}

class _OutlookBackend implements ComDispatchBackend {
  _OutlookBackend({this.mail, this.failReplyAll = false});

  final calls = <String>[];
  final _Obj? mail;
  final bool failReplyAll;
  bool initialized = false;
  bool uninitialized = false;
  bool outlookRunning = true;
  final List<_Obj> created = [];
  late final _Obj application = _Obj('Application', this);
  late final _Obj explorer = _Obj('Explorer', this);
  late final _Obj selection = _Obj('Selection', this);
  late final _Obj parent = _Obj('Parent', this);
  late final _Obj attachments = _Obj('Attachments', this);

  @override
  void initializeSta() {
    initialized = true;
    calls.add('init');
  }

  @override
  void uninitialize() {
    uninitialized = true;
    calls.add('uninit');
  }

  @override
  ComDispatch createApplication(String progId) =>
      throw const ComBackendFailure('create_excel', -1);

  @override
  ComDispatch getActiveObject(String progId) {
    calls.add('active:$progId');
    if (!outlookRunning) {
      throw const ComBackendFailure('active', 0x800401E3);
    }
    return application;
  }

  @override
  ComResult invoke(
    ComDispatch target,
    String name,
    DispatchInvokeKind kind,
    List<EncodedVariant> arguments,
    List<int> namedDispids,
  ) {
    final obj = target as _Obj;
    calls.add('$name:${kind.name}:${obj.name}');
    switch (name) {
      case 'ActiveExplorer':
        return explorer;
      case 'Selection':
        return selection;
      case 'Count':
        return obj == selection ? 1 : 0;
      case 'Item':
        if (obj == selection) {
          return mail ?? (throw const ComBackendFailure('item', -2));
        }
        return _unknown();
      case 'Session':
        return _Obj('Session', this);
      case 'GetItemFromID':
        if (mail == null) {
          throw const ComBackendFailure('get_item', -3);
        }
        return mail!;
      case 'ReplyAll':
        if (failReplyAll) {
          throw const ComBackendFailure('method:ReplyAll', -4);
        }
        final reply = _Obj('Reply', this);
        reply.props['BodyFormat'] = 1;
        reply.props['Body'] = 'original body';
        reply.props['HTMLBody'] = '<p>original</p>';
        reply.props['Saved'] = true;
        reply.props['Class'] = 43;
        reply.props['EntryID'] = 'reply-entry';
        reply.props['Subject'] = 'Re: subject';
        reply.props['Size'] = 10;
        reply.props['LastModificationTime'] = 45000.5;
        reply.props['To'] = '';
        reply.props['CC'] = '';
        reply.props['BCC'] = '';
        reply.props['Parent'] = parent;
        reply.props['Attachments'] = attachments;
        created.add(reply);
        return reply;
      case 'Display':
      case 'Delete':
        return true;
      case 'SaveAs':
        final path = (arguments.last as EncodedString).value;
        File(path).writeAsBytesSync([
          0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 1, 2, 3,
        ]);
        return true;
      default:
        break;
    }
    if (kind == DispatchInvokeKind.propertyPut) {
      obj.props[name] = (arguments.single as EncodedString).value;
      return true;
    }
    final value = obj.props[name];
    if (value == null) {
      if (name == 'StoreID') {
        return 'store';
      }
      throw ComBackendFailure('get:$name', -5);
    }
    return value;
  }

  _Obj _unknown() => _Obj('unknown', this);
}

_Obj _mailItem(_OutlookBackend backend) {
  final mail = _Obj('Mail', backend);
  mail.props['Class'] = 43;
  mail.props['EntryID'] = 'entry-1';
  mail.props['Subject'] = 'Subject line';
  mail.props['Body'] = 'Body text';
  mail.props['HTMLBody'] = '<p>Body</p>';
  mail.props['To'] = 'to@example.test';
  mail.props['CC'] = '';
  mail.props['BCC'] = '';
  mail.props['Size'] = 1234;
  mail.props['LastModificationTime'] = 45000.25;
  mail.props['Saved'] = true;
  mail.props['Parent'] = backend.parent;
  mail.props['Attachments'] = backend.attachments;
  return mail;
}

void main() {
  test('selected mail item returns identity, subject and stable fingerprint', () {
    final owner = _OutlookBackend();
    final mail = _mailItem(owner);
    final withMail = _OutlookBackend(mail: mail);
    final adapter = WindowsOutlookAdapter(
      backendFactory: () => withMail,
      tempRoot: Directory.systemTemp,
    );

    final first = adapter.selectedMailItem();
    final second = adapter.selectedMailItem();

    expect(first.identity, const MailItemIdentity(entryId: 'entry-1', storeId: 'store'));
    expect(first.subject, 'Subject line');
    expect(first.fingerprint.length, 64);
    expect(first.fingerprint, second.fingerprint);
    expect(withMail.uninitialized, isTrue);
  });

  test('a non-mail item is rejected before identity is read', () {
    final backend = _OutlookBackend();
    final mail = _mailItem(backend)..props['Class'] = 26;
    final adapter = WindowsOutlookAdapter(
      backendFactory: () => _OutlookBackend(mail: mail),
    );

    expect(
      adapter.selectedMailItem,
      throwsA(
        isA<OutlookFailure>().having(
          (failure) => failure.key,
          'key',
          'error.rfi.outlookDraft.mailItemRequired',
        ),
      ),
    );
  });

  test('Outlook not running is reported as unsupported', () {
    final backend = _OutlookBackend()..outlookRunning = false;
    final adapter = WindowsOutlookAdapter(backendFactory: () => backend);

    expect(
      adapter.selectedMailItem,
      throwsA(
        isA<OutlookFailure>()
            .having((f) => f.kind, 'kind', OutlookFailureKind.unsupported)
            .having((f) => f.key, 'key', 'error.rfi.outlookDraft.outlookNotRunning'),
      ),
    );
  });

  test('reply-all refuses a changed selection before calling ReplyAll', () {
    final backend = _OutlookBackend(mail: _mailItem(_OutlookBackend()));
    final adapter = WindowsOutlookAdapter(backendFactory: () => backend);

    expect(
      () => adapter.replyAll(
        const MailItemIdentity(entryId: 'other', storeId: 'store'),
      ),
      throwsA(
        isA<OutlookFailure>().having(
          (f) => f.key,
          'key',
          'error.rfi.outlookDraft.selectionChanged',
        ),
      ),
    );
    expect(backend.calls.contains('ReplyAll:method:Selection'), isFalse);
  });

  test('reply draft prepends plain text and reports a durable identity', () {
    final mail = _mailItem(_OutlookBackend());
    final backend = _OutlookBackend(mail: mail);
    final adapter = WindowsOutlookAdapter(backendFactory: () => backend);

    final draft = adapter.replyAll(
      const MailItemIdentity(entryId: 'entry-1', storeId: 'store'),
    );
    draft.prependBody(
      const ReplyContent(plainText: 'Plain', html: '<p>Html</p>'),
    );
    draft.display();
    final identity = draft.durableIdentity();

    final reply = backend.created.single;
    expect(reply.props['Body'], 'Plain\r\n\r\noriginal body');
    expect(identity, isNotNull);
    expect(identity!.entryId, 'reply-entry');
    expect(identity.storeId, 'store');
    draft.dispose();
    expect(backend.uninitialized, isTrue);
  });

  test('ReplyAll failure is outcome-unknown', () {
    final mail = _mailItem(_OutlookBackend());
    final backend = _OutlookBackend(mail: mail, failReplyAll: true);
    final adapter = WindowsOutlookAdapter(backendFactory: () => backend);

    expect(
      () => adapter.replyAll(
        const MailItemIdentity(entryId: 'entry-1', storeId: 'store'),
      ),
      throwsA(
        isA<OutlookFailure>()
            .having((f) => f.kind, 'kind', OutlookFailureKind.outcomeUnknown)
            .having((f) => f.key, 'key', 'error.rfi.outlookDraft.replyAllFailed'),
      ),
    );
  });

  test('MSG export returns a compound-file archive bound to the expected fingerprint', () {
    final mail = _mailItem(_OutlookBackend());
    final probe = WindowsOutlookAdapter(
      backendFactory: () => _OutlookBackend(mail: mail),
    );
    final identity = const MailItemIdentity(entryId: 'entry-1', storeId: 'store');
    final fingerprint = probe.selectedMailItem().fingerprint;
    final adapter = WindowsOutlookAdapter(
      backendFactory: () => _OutlookBackend(mail: mail),
    );

    final bytes = adapter.exportSelectedMsg(identity, fingerprint);

    expect(bytes.sublist(0, 8), [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
  });

  test('MSG export refuses a fingerprint that no longer matches', () {
    final mail = _mailItem(_OutlookBackend());
    final adapter = WindowsOutlookAdapter(
      backendFactory: () => _OutlookBackend(mail: mail),
    );

    expect(
      () => adapter.exportSelectedMsg(
        const MailItemIdentity(entryId: 'entry-1', storeId: 'store'),
        'not-the-fingerprint',
      ),
      throwsA(
        isA<OutlookFailure>().having(
          (f) => f.key,
          'key',
          'error.rfi.outlookArchive.selectionChanged',
        ),
      ),
    );
  });

  test('delete removes only the exact unchanged draft', () {
    final mail = _mailItem(_OutlookBackend());
    final probe = WindowsOutlookAdapter(
      backendFactory: () => _OutlookBackend(mail: mail),
    );
    final fingerprint = probe.selectedMailItem().fingerprint;
    final backend = _OutlookBackend(mail: mail);
    final adapter = WindowsOutlookAdapter(backendFactory: () => backend);

    adapter.deleteUnchangedDraft(
      DraftIdentity(
        entryId: 'entry-1',
        storeId: 'store',
        fingerprint: fingerprint,
      ),
    );

    expect(backend.calls.contains('Delete:method:Mail'), isTrue);
    expect(backend.uninitialized, isTrue);
  });
}
