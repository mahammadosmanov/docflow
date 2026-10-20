// Port of src-tauri/tests/gemini_http.rs.

import 'dart:convert';

import 'package:docflow/engine/auth.dart';
import 'package:docflow/engine/gemini.dart';
import 'package:docflow/engine/http.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e5_support.dart';

void main() {
  test(
    'gemini uses header secret and retries legacy structured schema',
    () async {
      final structured = <String, Object?>{
        'discipline': 'MEC',
        'count': 1,
        'rows': [
          <String, Object?>{
            'rfiIndex': 1,
            'itpNo': 'AZE-SUN-INT-QMS-ITP-MEC-0003',
            'itemNo': 'MEC-3.1',
            'tagNo': 'N3_PCS-02_S7/S8',
            'area': '',
            'location': '',
            'subcontractor': '',
          },
        ],
        'confidence': 0.9,
        'notes': <Object?>[],
      };
      final success = <String, Object?>{
        'candidates': [
          <String, Object?>{
            'content': <String, Object?>{
              'parts': [
                <String, Object?>{'text': jsonEncode(structured)},
              ],
            },
          },
        ],
      };
      final http = FixtureHttp([
        HttpResponse.fromJson(400, <String, Object?>{
          'error': <String, Object?>{'message': 'invalid'},
        }),
        HttpResponse.fromJson(400, <String, Object?>{
          'error': <String, Object?>{'message': 'invalid'},
        }),
        HttpResponse.fromJson(200, success),
      ]);
      final client = GeminiClient(http, 'http://gemini.test/v1beta', [
        'gemini-3.1-flash-lite',
      ]);
      final request = GeminiRequest('mail body', 'secret-key', 'MEC', {
        'AZE-SUN-INT-QMS-ITP-MEC-0003': ['MEC-3.1'],
      });

      final result = await client.extract(request);

      expect(result['provider'], 'gemini');
      expect(result['model'], 'gemini-3.1-flash-lite');
      expect(
        (result['rows']! as List<Object?>).first,
        containsPair('tagNo', 'N3_PCS-02_S7/S8'),
      );
      expect(http.requests, hasLength(3));
      expect(http.requests[0].url.contains('secret-key'), isFalse);
      expect(
        http.requests[0].hasHeader('x-goog-api-key', 'secret-key'),
        isTrue,
      );
      Map<String, Object?> config(int index) =>
          (http.requests[index].body!
                  as Map<String, Object?>)['generationConfig']!
              as Map<String, Object?>;
      expect(
        (config(0)['responseFormat']! as Map<String, Object?>)['text'],
        containsPair('schema', containsPair('type', 'object')),
      );
      expect(config(1)['responseJsonSchema'], containsPair('type', 'object'));
      expect(config(2)['responseSchema'], containsPair('type', 'OBJECT'));
      expect(http.requests[0].url.contains('secret-key'), isFalse);
    },
  );

  test('gemini retries the next model after not found', () async {
    final success = <String, Object?>{
      'candidates': [
        <String, Object?>{
          'content': <String, Object?>{
            'parts': [
              <String, Object?>{
                'text': jsonEncode(<String, Object?>{
                  'discipline': 'CIV',
                  'count': 0,
                  'rows': <Object?>[],
                  'confidence': 0.5,
                  'notes': <Object?>[],
                }),
              },
            ],
          },
        },
      ],
    };
    final http = FixtureHttp([
      HttpResponse.fromJson(404, null),
      HttpResponse.fromJson(200, success),
    ]);
    final client = GeminiClient(http, 'http://gemini.test/v1beta', [
      'retired-model',
      'gemini-3.1-flash-lite',
    ]);
    final request = GeminiRequest('mail body', 'secret-key', 'CIV', const {});

    final result = await client.extract(request);

    expect(result['model'], 'gemini-3.1-flash-lite');
    final urls = [for (final entry in http.requests) entry.url];
    expect(urls[0].contains('retired-model'), isTrue);
    expect(urls[1].contains('gemini-3.1-flash-lite'), isTrue);
  });

  test(
    'gemini validates the key, discipline and model names before any request',
    () async {
      final http = FixtureHttp(const []);
      final client = GeminiClient(http, 'http://gemini.test/v1beta', [
        'gemini-3.1-flash-lite',
      ]);

      Future<String> messageOf(GeminiRequest request) async {
        try {
          await client.extract(request);
        } on BackendError catch (error) {
          return error.message;
        }
        fail('expected a BackendError');
      }

      expect(
        await messageOf(GeminiRequest('mail', '  ', 'CIV', const {})),
        'Gemini API key is missing',
      );
      expect(
        await messageOf(GeminiRequest('mail', 'key', 'XYZ', const {})),
        'Gemini discipline is invalid',
      );
      final badModel = GeminiClient(http, 'http://gemini.test', ['bad model']);
      try {
        await badModel.extract(GeminiRequest('mail', 'key', 'CIV', const {}));
        fail('invalid model name must be rejected');
      } on BackendError catch (error) {
        expect(error.message, 'Gemini model name is invalid');
      }
      expect(http.requests, isEmpty);
    },
  );

  test(
    'gemini maps a non-retryable status to a stable message without the key',
    () async {
      final http = FixtureHttp([HttpResponse.fromJson(500, null)]);
      final client = GeminiClient(http, 'http://gemini.test/v1beta', [
        'gemini-3.1-flash-lite',
      ]);

      try {
        await client.extract(
          GeminiRequest('mail', 'secret-key', 'CIV', const {}),
        );
        fail('a 500 must fail');
      } on BackendError catch (error) {
        expect(error.code, 'backend_request_failed');
        expect(error.message, 'gemini_http_500');
        expect(error.message.contains('secret-key'), isFalse);
      }
    },
  );
}
