// Process gerektirmeyen saf birim testleri: sema ayristirma + i18n cozumleme.
// (Sidecar client / schema_form widget testleri Faz 2.2/2.3'te eklenecek.)

import 'package:docflow/core/l10n/strings.dart';
import 'package:docflow/core/operations/models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('OperationDef list_operations JSON semasini ayristirir', () {
    final op = OperationDef.fromJson({
      'id': 'rfi.process_scans',
      'titleKey': 'op.rfi.process_scans.title',
      'descriptionKey': 'op.rfi.process_scans.desc',
      'schema': [
        {
          'name': 'source',
          'kind': 'folder',
          'labelKey': 'param.source',
          'default': null,
          'required': true,
          'choices': [],
          'helpKey': 'param.source.help',
        },
        {
          'name': 'keep_original',
          'kind': 'bool',
          'labelKey': 'param.keep_original',
          'default': false,
          'required': false,
          'choices': [],
          'helpKey': null,
        },
      ],
    });

    expect(op.id, 'rfi.process_scans');
    expect(op.params.first.kind, ParamKind.folder);
    expect(op.params.first.required, isTrue);
    expect(op.params[1].kind, ParamKind.bool);
    expect(op.params[1].required, isFalse);
  });

  test('Plan.hasActionableSteps mkdir-only plani PASIF sayar', () {
    final onlyMkdir = Plan.fromJson({
      'planId': 'x',
      'opId': 'rfi.process_scans',
      'steps': [
        {'kind': 'mkdir', 'src': null, 'dst': '/out/A', 'noteKey': null, 'meta': {}},
      ],
      'warnings': [],
    });
    expect(onlyMkdir.hasActionableSteps, isFalse);

    final withMove = Plan.fromJson({
      'planId': 'x',
      'opId': 'rfi.process_scans',
      'steps': [
        {'kind': 'mkdir', 'src': null, 'dst': '/out/A', 'noteKey': null, 'meta': {}},
        {'kind': 'move', 'src': '/in/a.pdf', 'dst': '/out/A/a.pdf', 'noteKey': 'note.renamed', 'meta': {'confidence': 1.0}},
      ],
      'warnings': [],
    });
    expect(withMove.hasActionableSteps, isTrue);
    expect(withMove.steps[1].confidence, 1.0);
  });

  test('Strings cozulemeyen anahtari sessizce yutmaz, anahtari doner', () {
    final s = Strings({'action.apply': 'Apply'});
    expect(s.t('action.apply'), 'Apply');
    expect(s.t('yok.olan.anahtar'), 'yok.olan.anahtar');
    expect(Strings({'plan.summary': '{count} files'}).count('plan.summary', 3), '3 files');
  });
}
