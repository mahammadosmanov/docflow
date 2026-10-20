// Port of the inline tests in src-tauri/src/rfi_code.rs, plus the
// canonical-matching test from src-tauri/tests/process_scans.rs.
import 'package:flutter_test/flutter_test.dart';
import 'package:docflow/engine/rfi_code.dart' as rfi;

void main() {
  test('candidate number normalizes only one to four ASCII digits', () {
    expect(rfi.normalizeCandidateNumber('9'), '0009');
    expect(rfi.normalizeCandidateNumber('91'), '0091');
    expect(rfi.normalizeCandidateNumber('901'), '0901');
    expect(rfi.normalizeCandidateNumber('0901'), '0901');
    expect(rfi.normalizeCandidateNumber('09010'), isNull);
    expect(rfi.normalizeCandidateNumber('9O1'), isNull);
    expect(rfi.normalizeCandidateNumber(''), isNull);
  });

  test(
    'short suffixes are normalized only inside supported anchored RFI codes',
    () {
      final matched = rfi.find('AZE-SUN-INT-QMS-RFI-CIV-901');
      expect(matched.code, 'AZE-SUN-INT-QMS-RFI-CIV-0901');
      expect(
        rfi.normalizeFullCode('AZE-SUN-INT-QMS-RFI-CIV-901'),
        'AZE-SUN-INT-QMS-RFI-CIV-0901',
      );
      expect(rfi.normalizeFullCode('ITP-CIV-901'), isNull);
      expect(rfi.normalizeFullCode('AZE-SUN-INT-QMS-RFI-CIV-09010'), isNull);
      expect(rfi.exactCodes('AZE-SUN-INT-QMS-RFI-CIV-9'), {
        'AZE-SUN-INT-QMS-RFI-CIV-0009',
      });
      expect(rfi.exactCodes('ITP-CIV-901'), isEmpty);
      expect(rfi.exactCodes('2026-09-01'), isEmpty);
      expect(rfi.exactCodes('AZE-SUN-INT-QMS-RFI-XYZ-901'), isEmpty);
      expect(rfi.find('AZE-SUN-INT-QMS-RFI-CIV-09010').ok, isFalse);
    },
  );

  test(
    'recognizer rejects alphanumeric suffixes without truncating the identity',
    () {
      for (final suffix in ['901A', '901А', '09010']) {
        final value = 'AZE-SUN-INT-QMS-RFI-CIV-$suffix';
        final matched = rfi.find(value);
        expect(
          matched.ok,
          isFalse,
          reason: '$suffix was accepted as ${matched.code}',
        );
        expect(rfi.exactCodes(value), isEmpty, reason: '$suffix was extracted');
      }
      expect(
        rfi.find('AZE-SUN-INT-QMS-RFI-CIV-0901_EXTRA').code,
        'AZE-SUN-INT-QMS-RFI-CIV-0901',
      );
    },
  );

  test('wrapped short suffix requires a complete non-continued serial', () {
    final matched = rfi.find('AZE-SUN-INT-QMS-RFI-CIV-\n9');
    expect(matched.method, rfi.RfiMethod.strict);
    expect(matched.code, 'AZE-SUN-INT-QMS-RFI-CIV-0009');
    expect(rfi.find('AZE-SUN-INT-QMS-RFI-CIV-\n9\n01').ok, isFalse);
  });

  test('four digit identity survives ASCII OCR text concatenation', () {
    final matched = rfi.find('AZE-SUN-INT-QMS-RFI-CIV-0064REQUEST');
    expect(matched.method, rfi.RfiMethod.strict);
    expect(matched.code, 'AZE-SUN-INT-QMS-RFI-CIV-0064');
    expect(rfi.find('AZE-SUN-INT-QMS-RFI-CIV-901A').ok, isFalse);
    expect(rfi.find('AZE-SUN-INT-QMS-RFI-CIV-0064А').ok, isFalse);
    expect(rfi.find('AZE-SUN-INT-QMS-RFI-CIV-00645').ok, isFalse);
  });

  test('canonical RFI matching exposes the single normalized value', () {
    final (normalized, matched) = rfi.canonicalAndFind(
      'aze-sun-int-qms-rfi-civ-0064',
    );
    expect(normalized, 'AZE-SUN-INT-QMS-RFI-CIV-0064');
    expect(matched.code, normalized);
    expect(matched.method, rfi.RfiMethod.strict);
  });
}
