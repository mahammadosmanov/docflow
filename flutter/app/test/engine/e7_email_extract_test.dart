// Port of src-tauri/tests/email_extract.rs (offline extraction contract,
// Azerbaijani repair, bounded input and Gemini output range checks).
import 'package:docflow/engine/email_extract.dart';
import 'package:docflow/engine/errors.dart';
import 'package:flutter_test/flutter_test.dart';

EmailContext _context() => const EmailContext(
  discipline: 'MEC',
  itpToItems: {
    'AZE-SUN-INT-QMS-ITP-CIV-0005': ['CIV-5.2.1', 'CIV-5.3.1'],
    'AZE-SUN-INT-QMS-ITP-CIV-0007': ['CIV-7.1.1'],
    'AZE-SUN-INT-QMS-ITP-MEC-0003': ['MEC-3.1', 'MEC-3.2'],
  },
  knownTags: ['PCU-14', 'PCU-15', 'PCU-16'],
  knownSubcontractors: ['AZFEN'],
);

void main() {
  test('offline email contract matches count, mapping and duplicate rows', () {
    final extraction = offlineExtract(
      'Please create 1 CIV RFI number.\n'
      'ITP No: 5\nItem: CIV-5.2.1\nTag No: N12, S14\n'
      'Inspection Location: Foundation A\nSubcontractor: AZFEN',
      _context(),
    );

    expect(extraction.provider, 'offline');
    expect(extraction.discipline, 'CIV');
    expect(extraction.count, 1);
    expect(extraction.rows.map((row) => row.rfiIndex).toList(), [1, 1]);
    expect(extraction.rows.map((row) => row.tagNo).toList(), ['N12', 'S14']);
    expect(
      extraction.rows.every(
        (row) =>
            row.itpNo == 'AZE-SUN-INT-QMS-ITP-CIV-0005' &&
            row.itemNo == 'CIV-5.2.1' &&
            row.location == 'Foundation A',
      ),
      isTrue,
    );
  });

  test('offline email repairs Azerbaijani markdown and separator glyphs', () {
    final context = EmailContext(
      discipline: 'MEC',
      itpToItems: const {
        'AZE˗SUN˗INT˗QMS˗ITP˗CIV˗0005': ['CIV-5.2.1'],
        'AZE˗SUN˗INT˗QMS˗ITP˗MEC˗0003': ['MEC-3.3.1'],
      },
      knownTags: const ['PCU-14', 'PCU-15', 'PCU-16'],
      knownSubcontractors: const ['AZFEN'],
    );

    final civil = offlineExtract(
      'Transformator təməllərinin hazırlığı üçün **PCU-14/15/16** ərazilərinə\n'
      '(**AZE-SUN-INT-QMS-ITP-CIV-0005**) **3** ədəd **2.1** bəndi üzrə **RFI**',
      context,
    );
    final mechanical = offlineExtract(
      'PCS sahəsi üçün (AZE-SUN-INT-QMS-ITP-MEC-0003 əsasən) 3.1 bəndi üzrə\n'
      r'          1 ədəd RFI nömrəsi. N3\_PCS-02\_S7/S8',
      context,
    );

    expect(civil.discipline, 'CIV');
    expect(civil.count, 3);
    expect(civil.rows.map((row) => row.tagNo).toList(), [
      'PCU-14',
      'PCU-15',
      'PCU-16',
    ]);
    expect(civil.rows.every((row) => row.itemNo == 'CIV-5.2.1'), isTrue);
    expect(mechanical.discipline, 'MEC');
    expect(mechanical.rows.first.itemNo, 'MEC-3.3.1');
    expect(mechanical.rows.first.tagNo, 'N3_PCS-02_S7/S8');
  });

  test('extraction is bounded and malformed Gemini output is rejected', () {
    final text =
        'Create 100 CIV RFI numbers\nTag No: ${'N1,' * 500}\n'
        '${'x' * (maxEmailChars + 50)}';
    final extraction = offlineExtract(text, _context());
    expect(extraction.rows.length, lessThanOrEqualTo(100));
    expect(extraction.rows.every((row) => row.rfiIndex <= 100), isTrue);

    expect(
      () => validateGeminiExtraction({
        'discipline': 'CIV',
        'count': 101,
        'rows': <Object?>[],
        'confidence': 2.0,
        'notes': <Object?>[],
        'provider': 'gemini',
        'model': 'fixture-model',
      }),
      throwsA(isA<BackendError>().having((e) => e.code, 'code', 'validation')),
    );
  });
}
