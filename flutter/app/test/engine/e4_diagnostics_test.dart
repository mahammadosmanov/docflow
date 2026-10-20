// Port of the diagnostics.rs test module.
import 'dart:convert';

import 'package:docflow/engine/diagnostics.dart';
import 'package:docflow/engine/errors.dart';
import 'package:flutter_test/flutter_test.dart';

DiagnosticsInput _input(List<String> recentErrorCodes) => DiagnosticsInput(
  version: '0.6.0',
  platformOs: 'macos',
  platformArch: 'aarch64',
  ocrAvailable: true,
  ocrLanguageDataAvailable: true,
  recentErrorCodes: recentErrorCodes,
  printer: const PrinterDiagnostic(
    inventoryStatus: 'available',
    printerCount: 1,
    defaultPrinterAvailable: true,
  ),
);

Object? _decode(String content) => jsonDecode(content);

void main() {
  test('bundle shape is deterministic, bounded and pathless', () {
    final errors = [
      for (var index = maxRecentErrorCodes + 4; index >= 0; index--)
        'error_${index.toString().padLeft(2, '0')}',
    ];

    final first = buildBundle(_input(errors));
    final second = buildBundle(_input(errors));

    expect(first, second);
    expect(first.fileName, 'docflow-support-diagnostics-v1.json');
    expect(first.mediaType, 'application/json');
    expect(first.byteLength, utf8.encode(first.content).length);
    expect(first.byteLength <= maxBundleBytes, isTrue);
    expect(first.content.endsWith('\n'), isTrue);
    expect(_decode(first.content), {
      'schemaVersion': 1,
      'docflow': {'version': '0.6.0'},
      'platform': {'arch': 'aarch64', 'os': 'macos'},
      'components': {
        'ocr': {
          'engine': 'tesseract',
          'languageDataAvailable': true,
          'status': 'available',
        },
        'sidecar': {'required': false, 'status': 'retired'},
        'printer': {
          'inventoryStatus': 'available',
          'printerCount': 1,
          'defaultPrinterAvailable': true,
          'latestQueue': null,
        },
      },
      'recentErrorCodes': [
        for (var index = maxRecentErrorCodes + 4; index >= 5; index--)
          'error_${index.toString().padLeft(2, '0')}',
      ],
      'privacy': {'delivery': 'local-only', 'redaction': 'recursive'},
    });
    for (final rejected in ['/Users/', r'\Users\', 'accessToken', 'refreshToken']) {
      expect(first.content.contains(rejected), isFalse, reason: rejected);
    }
  });

  test('printer queue diagnostics are copy-valid and redact private paths', () {
    final value = DiagnosticsInput(
      version: '0.6.0',
      platformOs: 'macos',
      platformArch: 'aarch64',
      ocrAvailable: true,
      ocrLanguageDataAvailable: true,
      recentErrorCodes: const [],
      printer: PrinterDiagnostic(
        inventoryStatus: 'available',
        printerCount: 1,
        defaultPrinterAvailable: true,
        latestQueue: const PrintQueueDiagnostic(
          queueStatus: 'completed',
          printer: 'EPSON L15160 Series',
          fileCount: 1,
          counts: {'completed_unconfirmed': 1},
          jobs: [
            PrintJobDiagnostic(
              format: 'pdf',
              route: 'docflow',
              status: 'completed_unconfirmed',
              attempts: 1,
              paper: 'A4',
              orientation: 'auto',
              scaling: 'fit',
              color: 'printer',
              duplex: 'printer',
              pageRange: 'all',
              errorDetail: r'failed at C:\Users\Operator\private.pdf',
            ),
          ],
        ),
      ),
    );

    final bundle = buildBundle(value);

    expect(bundle.content.contains('EPSON L15160 Series'), isTrue);
    expect(bundle.content.contains('completed_unconfirmed'), isTrue);
    expect(bundle.content.contains('[REDACTED_PATH]'), isTrue);
    expect(bundle.content.contains('private.pdf'), isFalse);
    validateBundleContent(bundle.content);
  });

  test('recursive redaction replaces secrets, contents, paths and excess depth', () {
    final raw = <String, Object?>{
      'code': 'validation',
      'accessToken': 'access-value',
      'credentials': {'password': 'password-value'},
      'documentContents': 'private document text',
      'nested': [
        {
          'refresh_token': 'refresh-value',
          'message': 'failed at /Users/alice/Private/report.pdf',
          'filePath': r'C:\Users\Alice\Documents\report.pdf',
          'unc': r'\\server\private\report.pdf',
          'uri': 'file:///home/alice/report.pdf',
        },
      ],
    };
    Object? deep = 'hidden';
    for (var index = 0; index < 12; index++) {
      deep = {'next': deep};
    }
    raw['deep'] = deep;

    final sanitized =
        sanitizeDiagnosticValue(raw)! as Map<String, Object?>;
    final encoded = jsonEncode(sanitized);

    expect(sanitized['code'], 'validation');
    expect(sanitized['accessToken'], '[REDACTED_SECRET]');
    expect(sanitized['credentials'], '[REDACTED_SECRET]');
    expect(sanitized['documentContents'], '[REDACTED_CONTENT]');
    final nested = (sanitized['nested'] as List).first as Map;
    expect(nested['message'], '[REDACTED_PATH]');
    expect(nested['filePath'], '[REDACTED_PATH]');
    expect(nested['unc'], '[REDACTED_PATH]');
    expect(nested['uri'], '[REDACTED_PATH]');
    expect(encoded.contains('[REDACTED_DEPTH]'), isTrue);
    for (final rejected in [
      'access-value',
      'password-value',
      'private document text',
      '/Users/alice',
      r'C:\\Users',
      'server',
      '/home/alice',
      'hidden',
    ]) {
      expect(encoded.contains(rejected), isFalse, reason: rejected);
    }
  });

  test('recent error codes are newest first, bounded and sanitized', () {
    final log = RecentErrorCodes()..record('validation');
    log.record('contains /Users/alice/report.pdf');
    for (var index = 0; index < maxRecentErrorCodes + 2; index++) {
      log.record('safe_error_${index.toString().padLeft(2, '0')}');
    }

    final snapshot = log.snapshot();

    expect(snapshot, hasLength(maxRecentErrorCodes));
    expect(snapshot.first, 'safe_error_21');
    expect(snapshot.last, 'safe_error_02');
    expect(
      snapshot.every(
        (code) =>
            code.length <= 64 && RegExp(r'^[a-z0-9_.\-]+$').hasMatch(code),
      ),
      isTrue,
    );
    expect(snapshot.any((code) => code.contains('alice')), isFalse);
  });

  test('recorded error snapshot stays newest first in the built bundle', () {
    final log = RecentErrorCodes()
      ..record('older_error')
      ..record('newest_error');

    final bundle = buildBundle(_input(log.snapshot()));
    final value = _decode(bundle.content)! as Map<String, Object?>;

    expect(value['recentErrorCodes'], ['newest_error', 'older_error']);
  });

  test('copy validation accepts only canonical redacted bundle content', () {
    final bundle = buildBundle(_input(const ['validation']));
    validateBundleContent(bundle.content);

    final withSecret = _decode(bundle.content)! as Map<String, Object?>
      ..['accessToken'] = 'secret-value';
    final withSecretText = '${const JsonEncoder.withIndent('  ').convert(withSecret)}\n';
    expect(
      () => validateBundleContent(withSecretText),
      throwsA(
        isA<BackendError>().having((error) => error.code, 'code', 'validation'),
      ),
    );

    final withPath = bundle.content.replaceFirst('0.6.0', '/Users/alice/report.pdf');
    expect(
      () => validateBundleContent(withPath),
      throwsA(
        isA<BackendError>().having((error) => error.code, 'code', 'validation'),
      ),
    );
    expect(
      () => validateBundleContent('x' * (maxBundleBytes + 1)),
      throwsA(
        isA<BackendError>().having((error) => error.code, 'code', 'validation'),
      ),
    );
  });

  test('a bundle with a float schema version is rejected', () {
    final bundle = buildBundle(_input(const []));
    final changed = bundle.content.replaceFirst(
      '"schemaVersion": 1',
      '"schemaVersion": 1.0',
    );
    expect(
      () => validateBundleContent(changed),
      throwsA(isA<BackendError>()),
    );
  });
}

