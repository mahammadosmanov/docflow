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
    });

    expect(parameter.kind, ParamKind.files);
    expect(parameter.defaultValue, isEmpty);
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
}
