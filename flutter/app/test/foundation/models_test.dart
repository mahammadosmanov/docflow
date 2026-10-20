import 'package:docflow/models/models.dart';
import 'package:flutter_test/flutter_test.dart';

/// Each fixture is the wire form of one type in frontend/src/types.ts. The
/// round trip fromJson -> toJson must reproduce the fixture exactly.
void main() {
  test('ParamDef and OperationDef round trip', () {
    final param = {
      'name': 'destination',
      'kind': 'folder',
      'labelKey': 'param.destination',
      'default': null,
      'required': true,
      'choices': <String>[],
      'choiceLabels': <String, String>{},
      'visibleWhen': {'mode': 'copy'},
      'helpKey': 'help.destination',
      'remember': true,
    };
    expect(ParamDef.fromJson(param).toJson(), param);

    final operation = {
      'id': 'rfi.review',
      'titleKey': 'op.rfi.review.title',
      'descriptionKey': 'op.rfi.review.desc',
      'schema': [param],
      'supportsPresets': false,
    };
    expect(OperationDef.fromJson(operation).toJson(), operation);
    expect(ParamKind.fromWire('sheetCounts'), ParamKind.sheetCounts);
    expect(() => ParamKind.fromWire('nope'), throwsFormatException);
  });

  test('OperationPreset and EditorMetadata round trip', () {
    final preset = {
      'id': 'p1',
      'name': 'Morning',
      'values': {'count': 2},
    };
    expect(OperationPreset.fromJson(preset).toJson(), preset);
    final metadata = EditorMetadata.fromJson({'reorderable': true});
    expect(metadata.reorderable, isTrue);
    expect(EditorMetadata.fromJson({}).reorderable, isNull);
  });

  test('Plan, Step, Warning and OperationResult round trip', () {
    final plan = {
      'planId': 'plan-1',
      'opId': 'rfi.review',
      'steps': [
        {
          'kind': 'workbook_update',
          'src': '/a',
          'dst': '/b',
          'noteKey': 'note.moved',
          'meta': {'rows': 2},
        },
        {
          'kind': 'outlook_reply_draft',
          'src': null,
          'dst': null,
          'noteKey': null,
          'meta': <String, Object?>{},
        },
      ],
      'warnings': [
        {'key': 'warn.exists', 'subject': 'AZE-1'},
      ],
      'meta': {'total': 2},
    };
    expect(Plan.fromJson(plan).toJson(), plan);
    expect(Plan.fromJson(plan).steps.first.kind, StepKind.workbookUpdate);

    final result = {
      'applied': 2,
      'failed': 0,
      'journalId': 'j-1',
      'errors': <Object?>[],
      'meta': {'note': 'ok'},
      'cloudSynced': true,
    };
    expect(OperationResult.fromJson(result).toJson(), result);
  });

  test('auth and preference types round trip', () {
    final user = {
      'id': 'u1',
      'email': 'a@example.com',
      'username': 'ada',
      'display': 'Ada',
      'allowed': true,
      'isAdmin': false,
      'createdAt': '2026-01-01T00:00:00Z',
    };
    expect(AppUser.fromJson(user).toJson(), user);

    final auth = {
      'signedIn': true,
      'allowed': true,
      'isAdmin': false,
      'session': {'userId': 'u1', 'email': 'a@example.com', 'expiresAt': 1.7e9},
      'user': user,
    };
    expect(AuthState.fromJson(auth).toJson(), auth);

    final prefs = {
      'locale': 'az',
      'theme': 'dark',
      'font': 'mono',
      'fontScale': 1.25,
      'compact': true,
      'startWithWindows': false,
      'replicaSyncEnabled': true,
    };
    expect(AppPrefs.fromJson(prefs).toJson(), prefs);
    expect(AppPrefs.fromJson({...prefs, 'locale': null}).locale, isNull);
  });

  test('AppPrefsPatch keeps an explicit null locale only when cleared', () {
    expect(const AppPrefsPatch(compact: true).toJson(), {'compact': true});
    expect(const AppPrefsPatch(clearLocale: true).toJson(), {'locale': null});
    expect(const AppPrefsPatch(locale: AppLocale.ru).toJson(), {
      'locale': 'ru',
    });
  });

  test('Bootstrap round trip', () {
    final bootstrap = {
      'protocol': bootstrapProtocolVersion,
      'version': '2.0.0',
      'auth': AuthState.signedOut.toJson(),
      'prefs': AppPrefs.defaults.toJson(),
      'operations': <Object?>[],
    };
    expect(Bootstrap.fromJson(bootstrap).toJson(), bootstrap);
  });

  test('replica, print queue and recovery types round trip', () {
    final replica = {
      'enabled': true,
      'configured': true,
      'status': 'waitingForSignIn',
      'lastSuccessUnix': 1700000000,
      'lastErrorCode': null,
    };
    expect(ReplicaAutomationStatus.fromJson(replica).toJson(), replica);

    final queue = {
      'queueId': 'q1',
      'status': 'paused',
      'createdAt': '2026-01-01',
      'updatedAt': '2026-01-02',
      'counts': {'queued': 2, 'done': 1},
      'resumable': 2,
      'fileCount': 3,
    };
    expect(PrintQueueSummary.fromJson(queue).toJson(), queue);

    final recovery = {
      'journals': [
        {
          'journalId': 'j1',
          'disposition': 'outcome_unknown',
          'restoreOriginalAvailable': true,
        },
      ],
      'printQueue': queue,
    };
    expect(RecoveryPayload.fromJson(recovery).toJson(), recovery);
    expect(
      RecoveryDisposition.fromWire('no_effect'),
      RecoveryDisposition.noEffect,
    );
  });

  test('review, history and path types round trip', () {
    final selection = {
      'selectionId': 's1',
      'displayPath': '~/RFI/AZE-1.pdf',
      'name': 'AZE-1.pdf',
    };
    expect(PathSelection.fromJson(selection).toJson(), selection);

    final item = {
      'selectionId': 's1',
      'path': '/review/AZE-1.pdf',
      'name': 'AZE-1.pdf',
      'code': 'AZE-SUN-INT-QMS-RFI-CIV-0001',
      'confidence': 0.92,
      'targetExists': false,
    };
    expect(ReviewItem.fromJson(item).toJson(), item);

    final decision = {'itemId': 's1', 'action': 'put', 'code': 'AZE-1'};
    expect(ReviewDecision.fromJson(decision).toJson(), decision);
    expect(
      () => ReviewDecision.fromJson({...decision, 'action': 'keep'}),
      throwsFormatException,
    );

    final row = {
      'when': '2026-01-01T10:00:00Z',
      'opId': 'rfi.review',
      'moves': 3,
      'failed': 0,
      'journalId': 'j1',
      'steps': null,
      'cloudId': null,
      'undone': false,
      'undoAvailable': true,
      'fromCloud': false,
      'userEmail': 'a@example.com',
    };
    final payload = {
      'online': false,
      'rows': [row],
    };
    expect(HistoryPayload.fromJson(payload).toJson(), payload);
  });

  test('RFI print find types round trip', () {
    final selection = {
      'selectionId': 's1',
      'displayPath': 'x.pdf',
      'name': 'x.pdf',
    };
    final candidate = {
      'requested': '12',
      'rfiCode': 'AZE-SUN-INT-QMS-RFI-CIV-0012',
      'documentType': 'firstPage',
      'displayPath': 'x.pdf',
      'selection': selection,
      'selected': true,
    };
    final resolution = {
      'requested': '12',
      'rfiCodes': ['AZE-SUN-INT-QMS-RFI-CIV-0012'],
      'missingFirstPage': false,
      'missingSketch': true,
      'ambiguous': false,
    };
    final find = {
      'candidates': [candidate],
      'resolutions': [resolution],
      'visited': 9,
      'truncated': false,
    };
    expect(RfiPrintFindResult.fromJson(find).toJson(), find);
  });

  test('progress and support bundle types round trip', () {
    final progress = {'phase': 'scan', 'done': 1, 'total': 4, 'subject': 'x'};
    expect(ProgressEvent.fromJson(progress).toJson(), progress);

    final bundle = {
      'fileName': 'diagnostics.json',
      'mediaType': 'application/json',
      'byteLength': 12,
      'content': '{"a":1}',
    };
    expect(SupportBundlePreview.fromJson(bundle).toJson(), bundle);
    expect(
      () =>
          SupportBundlePreview.fromJson({...bundle, 'mediaType': 'text/plain'}),
      throwsFormatException,
    );
  });

  test('a missing required field is a FormatException', () {
    expect(
      () => PathSelection.fromJson({'selectionId': 's1', 'name': 'x'}),
      throwsFormatException,
    );
  });
}
