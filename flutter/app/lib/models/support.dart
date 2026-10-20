import 'json.dart';

/// Diagnostics bundle preview before the user copies or saves it
/// (`SupportBundlePreview`).
final class SupportBundlePreview {
  const SupportBundlePreview({
    required this.fileName,
    required this.byteLength,
    required this.content,
  });

  /// The only media type the engine produces for diagnostics.
  static const String mediaType = 'application/json';

  factory SupportBundlePreview.fromJson(JsonMap json) {
    final media = jsonString(json, 'mediaType');
    if (media != mediaType) {
      throw FormatException('Unexpected mediaType "$media"');
    }
    return SupportBundlePreview(
      fileName: jsonString(json, 'fileName'),
      byteLength: jsonInt(json, 'byteLength'),
      content: jsonString(json, 'content'),
    );
  }

  final String fileName;
  final int byteLength;
  final String content;

  JsonMap toJson() => {
    'fileName': fileName,
    'mediaType': mediaType,
    'byteLength': byteLength,
    'content': content,
  };
}
