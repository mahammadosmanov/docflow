// Port of src-tauri/src/http.rs.
//
// Bounded JSON-over-HTTP transport. Rust used ureq; Dart uses package:http with
// dart:io's HttpClient (connect timeout). Every failure is reduced to a stable
// [HttpError] so that no response body, URL secret or header value can leak
// into an error message.

import 'dart:async';
import 'dart:convert';
import 'dart:io' as io;
import 'dart:typed_data';

import 'package:http/http.dart' as http;
import 'package:http/io_client.dart' as http_io;

class HttpConfig {
  const HttpConfig({
    this.connectTimeout = const Duration(seconds: 5),
    this.requestTimeout = const Duration(seconds: 15),
    this.maxRequestBytes = 512 * 1024,
    this.maxResponseBytes = 2 * 1024 * 1024,
  });

  final Duration connectTimeout;
  final Duration requestTimeout;
  final int maxRequestBytes;
  final int maxResponseBytes;
}

enum HttpError {
  invalidRequest,
  requestTooLarge,
  responseTooLarge,
  timeout,
  offline,
  invalidResponse;

  /// Same wording as the Rust `Display` implementation.
  String get message => switch (this) {
    HttpError.invalidRequest => 'network request is invalid',
    HttpError.requestTooLarge => 'network request exceeded the size limit',
    HttpError.responseTooLarge => 'network response exceeded the size limit',
    HttpError.timeout => 'network request timed out',
    HttpError.offline => 'network is unavailable',
    HttpError.invalidResponse => 'network response is invalid',
  };
}

/// Thrown by [JsonHttp.sendJson] in place of a Rust `Err(HttpError)`.
class HttpFailure implements Exception {
  const HttpFailure(this.error);

  final HttpError error;

  @override
  String toString() => 'HttpFailure(${error.name})';
}

class HttpResponse {
  HttpResponse(this.status, this._body);

  factory HttpResponse.fromJson(int status, Object? value) =>
      HttpResponse(status, Uint8List.fromList(utf8.encode(jsonEncode(value))));

  final int status;
  final Uint8List _body;

  /// An empty body is JSON `null`, as in the Rust `json()` helper.
  Object? json() {
    if (_body.isEmpty) {
      return null;
    }
    try {
      return jsonDecode(utf8.decode(_body));
    } on FormatException {
      throw const HttpFailure(HttpError.invalidResponse);
    }
  }
}

/// Seam used by auth, cloud and gemini. Production code uses [HttpClient];
/// tests inject fixtures. The `body` is a JSON value, or `null` for no body.
abstract interface class JsonHttp {
  Future<HttpResponse> sendJson(
    String method,
    String url,
    List<(String, String)> headers,
    Object? body,
  );
}

class HttpClient implements JsonHttp {
  HttpClient._(this._config, this._client);

  /// Validates [config] like `HttpClient::new` and returns a client.
  /// Throws [HttpFailure] with [HttpError.invalidRequest] for zero bounds.
  factory HttpClient(HttpConfig config, {http.Client? client}) {
    if (config.connectTimeout == Duration.zero ||
        config.requestTimeout == Duration.zero ||
        config.maxRequestBytes <= 0 ||
        config.maxResponseBytes <= 0) {
      throw const HttpFailure(HttpError.invalidRequest);
    }
    final transport =
        client ??
        http_io.IOClient(
          io.HttpClient()..connectionTimeout = config.connectTimeout,
        );
    return HttpClient._(config, transport);
  }

  final HttpConfig _config;
  final http.Client _client;

  /// Releases the underlying transport.
  void close() => _client.close();

  @override
  Future<HttpResponse> sendJson(
    String method,
    String url,
    List<(String, String)> headers,
    Object? body,
  ) {
    return _sendJson(method, url, headers, body).timeout(
      _config.requestTimeout,
      onTimeout: () => throw const HttpFailure(HttpError.timeout),
    );
  }

  Future<HttpResponse> _sendJson(
    String method,
    String url,
    List<(String, String)> headers,
    Object? body,
  ) async {
    final encoded = body == null ? null : _encodeBody(body);
    if (encoded != null && encoded.length > _config.maxRequestBytes) {
      throw const HttpFailure(HttpError.requestTooLarge);
    }

    if (!_isToken(method)) {
      throw const HttpFailure(HttpError.invalidRequest);
    }
    final uri = Uri.tryParse(url);
    if (uri == null ||
        !(uri.isScheme('http') || uri.isScheme('https')) ||
        uri.host.isEmpty) {
      throw const HttpFailure(HttpError.invalidRequest);
    }

    // Header merge is last-writer-wins: a Dart map cannot carry the duplicate
    // header lines that the Rust builder would append.
    final headerMap = <String, String>{
      'Accept': 'application/json',
      if (encoded != null) 'Content-Type': 'application/json',
    };
    for (final (name, value) in headers) {
      if (!_isToken(name) || !_isValidHeaderValue(value)) {
        throw const HttpFailure(HttpError.invalidRequest);
      }
      headerMap[name] = value;
    }

    final request = http.Request(method, uri)
      ..followRedirects = false
      ..maxRedirects = 0
      ..headers.addAll(headerMap);
    if (encoded != null) {
      request.bodyBytes = encoded;
    }

    final http.StreamedResponse streamed;
    try {
      streamed = await _client.send(request);
    } on HttpFailure {
      rethrow;
    } catch (error) {
      throw HttpFailure(_mapTransportError(error));
    }

    final maximum = _config.maxResponseBytes;
    final declared = streamed.contentLength;
    if (declared != null && declared > maximum) {
      await _drain(streamed);
      throw const HttpFailure(HttpError.responseTooLarge);
    }
    final bytes = BytesBuilder(copy: false);
    try {
      await for (final chunk in streamed.stream) {
        bytes.add(chunk);
        if (bytes.length > maximum) {
          throw const HttpFailure(HttpError.responseTooLarge);
        }
      }
    } on HttpFailure {
      rethrow;
    } catch (error) {
      throw HttpFailure(_mapTransportError(error));
    }
    return HttpResponse(streamed.statusCode, bytes.takeBytes());
  }

  static Uint8List _encodeBody(Object body) {
    try {
      return Uint8List.fromList(utf8.encode(jsonEncode(body)));
    } on JsonUnsupportedObjectError {
      throw const HttpFailure(HttpError.invalidRequest);
    }
  }

  static Future<void> _drain(http.StreamedResponse response) async {
    try {
      await response.stream.drain<void>();
    } catch (_) {
      // The response is already rejected; a failed drain changes nothing.
    }
  }
}

/// RFC 9110 token characters, matching `http::Method` and header names.
final RegExp _tokenPattern = RegExp(r"^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$");

bool _isToken(String value) => _tokenPattern.hasMatch(value);

/// Rejects control characters other than horizontal tab, as the `http` crate does.
bool _isValidHeaderValue(String value) {
  for (final unit in value.codeUnits) {
    if ((unit < 0x20 && unit != 0x09) || unit == 0x7f) {
      return false;
    }
  }
  return true;
}

/// Maps transport failures onto the stable set used by ureq's `map_ureq_error`.
HttpError _mapTransportError(Object error) {
  if (error is TimeoutException) {
    return HttpError.timeout;
  }
  if (error is io.HttpException) {
    return HttpError.invalidResponse;
  }
  if (error is FormatException) {
    return HttpError.invalidResponse;
  }
  return HttpError.offline;
}
