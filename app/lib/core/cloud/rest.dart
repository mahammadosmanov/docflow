import 'dart:convert';
import 'dart:io';

/// Minimal HTTPS yardimcisi — dart:io, yeni bagimlilik YOK.
/// (int statusCode, String body) doner.
Future<(int, String)> restSend(
  String method,
  Uri uri, {
  Map<String, String> headers = const {},
  Object? body,
}) async {
  final c = HttpClient()..connectionTimeout = const Duration(seconds: 15);
  try {
    final req = await c.openUrl(method, uri);
    req.headers.set('Content-Type', 'application/json');
    headers.forEach(req.headers.set);
    if (body != null) req.write(json.encode(body));
    final res = await req.close();
    final text = await res.transform(utf8.decoder).join();
    return (res.statusCode, text);
  } finally {
    c.close(force: true);
  }
}
