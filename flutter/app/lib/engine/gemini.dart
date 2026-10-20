// Port of src-tauri/src/gemini.rs.
//
// Structured RFI email extraction through the Gemini generateContent API. The
// API key travels only in the x-goog-api-key header and never in a URL or in
// an error message. Requests try three structured-output formats, then the
// next model when a model is missing (HTTP 404).

import 'dart:convert';

import 'auth.dart';
import 'http.dart';

const String defaultGeminiModel = 'gemini-3.1-flash-lite';
const List<String> fallbackGeminiModels = <String>[
  'gemini-3.5-flash-lite',
  'gemini-3.5-flash',
];
const int _maxEmailChars = 30000;
const int _maxApiKeyChars = 512;
const int _maxModelChars = 128;

/// One extraction request.
///
/// UNVERIFIED: Rust zeroizes the API key on drop. Dart strings are immutable and
/// garbage-collected, so the key cannot be wiped from memory here. It is held
/// only by this object and is never placed in a URL, error or result.
final class GeminiRequest {
  GeminiRequest(this.text, this._apiKey, this.discipline, this.itpToItems);

  final String text;
  final String _apiKey;
  final String discipline;
  final Map<String, List<String>> itpToItems;
}

final class GeminiClient {
  GeminiClient(this._http, String baseUrl, List<String> models)
    : _baseUrl = baseUrl.replaceAll(RegExp(r'/+$'), ''),
      _models = List<String>.unmodifiable(models);

  final JsonHttp _http;
  final String _baseUrl;
  final List<String> _models;

  /// Returns the decoded structured object plus `provider` and `model` keys.
  Future<Map<String, Object?>> extract(GeminiRequest request) async {
    final key = request._apiKey.trim();
    if (key.isEmpty) {
      throw BackendError.validation('Gemini API key is missing');
    }
    if (key.runes.length > _maxApiKeyChars) {
      throw BackendError.validation('Gemini API key exceeded the size limit');
    }
    if (!const {'CIV', 'MEC', 'ELE', 'PRC'}.contains(request.discipline)) {
      throw BackendError.validation('Gemini discipline is invalid');
    }
    if (_models.isEmpty || _models.any((model) => !_validModel(model))) {
      throw BackendError.validation('Gemini model name is invalid');
    }

    final prompt = _prompt(
      request.text,
      request.discipline,
      request.itpToItems,
    );
    final baseBody = <String, Object?>{
      'contents': <Object?>[
        <String, Object?>{
          'role': 'user',
          'parts': <Object?>[
            <String, Object?>{'text': prompt},
          ],
        },
      ],
    };
    final formats = _requestFormats();
    final headers = <(String, String)>[('x-goog-api-key', key)];
    var sawInvalidFormat = false;
    var sawMissingModel = false;

    for (final model in _models) {
      for (final generationConfig in formats) {
        final body = <String, Object?>{
          ...baseBody,
          'generationConfig': generationConfig,
        };
        final url = '$_baseUrl/models/$model:generateContent';
        final HttpResponse response;
        try {
          response = await _http.sendJson('POST', url, headers, body);
        } on HttpFailure catch (failure) {
          throw _mapHttpError(failure.error);
        }
        final status = response.status;
        if (status >= 200 && status <= 299) {
          final Object? payload;
          try {
            payload = response.json();
          } on HttpFailure {
            throw BackendError.requestFailed(
              'Gemini returned an invalid structured response',
            );
          }
          return _decodeResult(payload, model);
        }
        if (status == 404) {
          sawMissingModel = true;
          break;
        }
        if (status == 400) {
          sawInvalidFormat = true;
          continue;
        }
        throw BackendError.requestFailed('gemini_http_$status');
      }
    }
    if (sawInvalidFormat) {
      throw BackendError.requestFailed(
        'Gemini rejected all supported request formats',
      );
    }
    if (sawMissingModel) {
      throw BackendError.requestFailed(
        'No supported Gemini model was available',
      );
    }
    throw BackendError.requestFailed('Gemini request failed');
  }
}

/// Builds the prompt. The string is assembled from pieces whose spacing matches
/// the Rust source. The mapping is serialized with its keys sorted, as the
/// Rust BTreeMap is.
String _prompt(
  String text,
  String discipline,
  Map<String, List<String>> itpToItems,
) {
  final sorted = Map<String, List<String>>.fromEntries(
    itpToItems.entries.toList()..sort((a, b) => a.key.compareTo(b.key)),
  );
  final mapping = jsonEncode(sorted);
  final email = String.fromCharCodes(text.runes.take(_maxEmailChars));
  return 'Extract an RFI inspection request from the email below. '
      'Use only facts explicitly present in the email; never invent values. '
      'Discipline must be CIV, MEC, ELE, or PRC; use the currently selected discipline '
      'when the email does not state one. Other unknown strings must be empty. '
      'Count means unique new RFI numbers, not Excel rows. When several tags/areas are requested '
      'for the same RFI, return multiple rows with the same rfiIndex, but keep the unique RFI '
      'count stated by the sender. rfiIndex is 1-based and identifies which new RFI number each '
      'Excel row belongs to. ITP and Item values must exactly match this allowed mapping: '
      '$mapping. The currently selected discipline is $discipline. '
      'Return short review notes for ambiguity.\n\nEMAIL:\n$email';
}

/// Three request shapes, tried in order: the current `responseFormat`, the
/// `responseJsonSchema` shape, and the legacy upper-case `responseSchema`.
List<Map<String, Object?>> _requestFormats() {
  final currentSchema = _responseSchema(legacy: false);
  final legacySchema = _responseSchema(legacy: true);
  return <Map<String, Object?>>[
    <String, Object?>{
      'responseFormat': <String, Object?>{
        'text': <String, Object?>{
          'mimeType': 'APPLICATION_JSON',
          'schema': currentSchema,
        },
      },
    },
    <String, Object?>{
      'responseMimeType': 'application/json',
      'responseJsonSchema': currentSchema,
    },
    <String, Object?>{
      'responseMimeType': 'application/json',
      'responseSchema': legacySchema,
    },
  ];
}

Map<String, Object?> _responseSchema({required bool legacy}) {
  final object = legacy ? 'OBJECT' : 'object';
  final string = legacy ? 'STRING' : 'string';
  final integer = legacy ? 'INTEGER' : 'integer';
  final number = legacy ? 'NUMBER' : 'number';
  final array = legacy ? 'ARRAY' : 'array';
  final row = <String, Object?>{
    'type': object,
    'properties': <String, Object?>{
      'rfiIndex': <String, Object?>{
        'type': integer,
        'minimum': 1,
        'maximum': 100,
      },
      'itpNo': <String, Object?>{'type': string},
      'itemNo': <String, Object?>{'type': string},
      'tagNo': <String, Object?>{'type': string},
      'area': <String, Object?>{'type': string},
      'location': <String, Object?>{'type': string},
      'subcontractor': <String, Object?>{'type': string},
    },
    'required': <String>[
      'rfiIndex',
      'itpNo',
      'itemNo',
      'tagNo',
      'area',
      'location',
      'subcontractor',
    ],
  };
  return <String, Object?>{
    'type': object,
    'properties': <String, Object?>{
      'discipline': <String, Object?>{
        'type': string,
        'enum': <String>['CIV', 'MEC', 'ELE', 'PRC'],
      },
      'count': <String, Object?>{'type': integer, 'minimum': 0, 'maximum': 100},
      'rows': <String, Object?>{'type': array, 'items': row, 'maxItems': 100},
      'confidence': <String, Object?>{
        'type': number,
        'minimum': 0,
        'maximum': 1,
      },
      'notes': <String, Object?>{
        'type': array,
        'items': <String, Object?>{'type': string},
      },
    },
    'required': <String>['discipline', 'count', 'rows', 'confidence', 'notes'],
  };
}

Map<String, Object?> _decodeResult(Object? payload, String model) {
  final encoded = _candidateText(payload);
  if (encoded == null) {
    throw BackendError.requestFailed(
      'Gemini returned an invalid structured response',
    );
  }
  Object? parsed;
  try {
    parsed = jsonDecode(encoded);
  } on FormatException {
    parsed = null;
  }
  if (parsed is! Map<String, Object?>) {
    throw BackendError.requestFailed('Gemini response is not an object');
  }
  return <String, Object?>{...parsed, 'provider': 'gemini', 'model': model};
}

String? _candidateText(Object? payload) {
  if (payload is! Map<String, Object?>) {
    return null;
  }
  final candidates = payload['candidates'];
  final first = candidates is List<Object?> && candidates.isNotEmpty
      ? candidates.first
      : null;
  final content = first is Map<String, Object?> ? first['content'] : null;
  final parts = content is Map<String, Object?> ? content['parts'] : null;
  final part = parts is List<Object?> && parts.isNotEmpty ? parts.first : null;
  final text = part is Map<String, Object?> ? part['text'] : null;
  return text is String ? text : null;
}

bool _validModel(String model) {
  if (model.isEmpty || model.runes.length > _maxModelChars) {
    return false;
  }
  for (final byte in model.codeUnits) {
    final alphanumeric =
        (byte >= 0x30 && byte <= 0x39) ||
        (byte >= 0x41 && byte <= 0x5a) ||
        (byte >= 0x61 && byte <= 0x7a);
    final separator = byte == 0x2e || byte == 0x5f || byte == 0x2d;
    if (!alphanumeric && !separator) {
      return false;
    }
  }
  return true;
}

BackendError _mapHttpError(HttpError error) => switch (error) {
  HttpError.timeout => BackendError.networkTimeout(),
  HttpError.responseTooLarge => BackendError.responseTooLarge(),
  HttpError.offline => BackendError.offline(),
  _ => BackendError.requestFailed(error.message),
};
