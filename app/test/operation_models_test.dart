import 'package:docflow/core/operations/models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('multi-file parameters are parsed from the operation schema', () {
    final parameter = ParamDef.fromJson({
      'name': 'files',
      'kind': 'files',
      'labelKey': 'param.print.files',
      'required': true,
      'default': <String>[],
      'itemChoices': ['A4', 'A3'],
      'itemChoiceLabels': {
        'A4': 'choice.print.paper.a4',
        'A3': 'choice.print.paper.a3',
      },
      'itemDefault': 'A4',
    });

    expect(parameter.kind, ParamKind.files);
    expect(parameter.defaultValue, isEmpty);
    expect(parameter.itemChoices, ['A4', 'A3']);
    expect(parameter.itemChoiceLabels['A3'], 'choice.print.paper.a3');
    expect(parameter.itemDefault, 'A4');
  });

  test('multi-sheet RFI counts are parsed from the operation schema', () {
    final parameter = ParamDef.fromJson({
      'name': 'sheet_counts',
      'kind': 'sheetCounts',
      'labelKey': 'param.rfi.sheetCounts',
      'required': true,
      'default': {'CIV': 3, 'MEC': 0, 'ELE': 0, 'PRC': 0},
      'choices': ['CIV', 'MEC', 'ELE', 'PRC'],
    });

    expect(parameter.kind, ParamKind.sheetCounts);
    expect(parameter.choices, ['CIV', 'MEC', 'ELE', 'PRC']);
    expect(parameter.defaultValue, {'CIV': 3, 'MEC': 0, 'ELE': 0, 'PRC': 0});
  });

  test('a print step makes a preview plan actionable', () {
    final plan = Plan.fromJson({
      'planId': 'preview-1',
      'opId': 'files.batch_print',
      'steps': [
        {
          'kind': 'print',
          'src': r'C:\Docs\sample.pdf',
          'dst': r'C:\Docs\sample.pdf',
          'meta': {'printer': 'Office Printer'},
        },
      ],
      'warnings': <Map<String, dynamic>>[],
      'meta': {'nonUndoable': true},
    });

    expect(plan.hasActionableSteps, isTrue);
  });

  test('guided print session exposes the current file and recovery state', () {
    final session = GuidedSession.fromJson({
      'sessionId': 'queue-1',
      'revision': 7,
      'opId': 'files.batch_print',
      'status': 'paused',
      'current': 1,
      'total': 3,
      'currentJob': {
        'path': r'C:\Docs\second.pdf',
        'paperSize': 'A3',
        'copies': 2,
      },
    });

    expect(session.isPaused, isTrue);
    expect(session.revision, 7);
    expect(session.isComplete, isFalse);
    expect(session.currentNumber, 2);
    expect(session.currentJob?['paperSize'], 'A3');
  });

  test('apply result carries a guided session', () {
    final result = ApplyResult.fromJson({
      'applied': 0,
      'failed': 0,
      'journalId': '',
      'errors': <Object>[],
      'meta': {
        'guidedSession': {
          'sessionId': 'queue-1',
          'revision': 0,
          'opId': 'files.batch_print',
          'status': 'active',
          'current': 0,
          'total': 2,
          'currentJob': {
            'path': r'C:\Docs\first.pdf',
            'paperSize': 'A4',
            'copies': 1,
          },
        },
      },
    });

    expect(result.guidedSession?.sessionId, 'queue-1');
    expect(result.guidedSession?.total, 2);
  });

  test('an unfinished guided session must identify its current job', () {
    expect(
      () => GuidedSession.fromJson({
        'sessionId': 'queue-1',
        'status': 'paused',
        'current': 0,
        'total': 1,
        'currentJob': null,
      }),
      throwsFormatException,
    );

    expect(
      () => GuidedSession.fromJson({
        'sessionId': 'queue-1',
        'opId': 'files.batch_print',
        'status': 'completed',
        'current': 0,
        'total': 1,
        'currentJob': null,
      }),
      throwsFormatException,
    );
  });

  test('guided session rejects values outside the wire contract', () {
    final valid = <String, dynamic>{
      'sessionId': 'queue-1',
      'revision': 3,
      'opId': 'files.batch_print',
      'status': 'paused',
      'current': 0,
      'total': 1,
      'currentJob': {
        'path': r'C:\Docs\first.pdf',
        'paperSize': 'A4',
        'copies': 1,
      },
      'lastError': '',
    };
    final invalidValues = <String, Object?>{
      'revision': '3',
      'opId': 'other.operation',
      'lastError': 1,
    };

    for (final entry in invalidValues.entries) {
      expect(
        () => GuidedSession.fromJson({...valid, entry.key: entry.value}),
        throwsFormatException,
      );
    }
    for (final job in [
      {'path': 'relative.pdf', 'paperSize': 'A4', 'copies': 1},
      {'path': r'C:\Docs\first.pdf', 'paperSize': 'A9', 'copies': 1},
      {'path': r'C:\Docs\first.pdf', 'paperSize': 'A4', 'copies': 0},
    ]) {
      expect(
        () => GuidedSession.fromJson({...valid, 'currentJob': job}),
        throwsFormatException,
      );
    }
  });
}
