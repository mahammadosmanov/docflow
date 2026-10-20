import 'json.dart';

/// Review actions shown in the UI (`ReviewAction`).
enum ReviewAction {
  keep,
  put,
  delete;

  static ReviewAction fromWire(String value) => ReviewAction.values.firstWhere(
    (action) => action.name == value,
    orElse: () => throw FormatException('Unknown ReviewAction "$value"'),
  );
}

/// The actions that can be sent to the engine: `Exclude<ReviewAction, "keep">`.
enum ReviewDecisionAction {
  put,
  delete;

  static ReviewDecisionAction fromWire(String value) =>
      ReviewDecisionAction.values.firstWhere(
        (action) => action.name == value,
        orElse: () =>
            throw FormatException('Invalid ReviewDecision action "$value"'),
      );
}

/// A file found in the review folder (`ReviewItem`).
final class ReviewItem {
  const ReviewItem({
    required this.selectionId,
    required this.path,
    required this.name,
    required this.code,
    required this.confidence,
    this.targetExists,
  });

  factory ReviewItem.fromJson(JsonMap json) => ReviewItem(
    selectionId: jsonString(json, 'selectionId'),
    path: jsonString(json, 'path'),
    name: jsonString(json, 'name'),
    code: jsonString(json, 'code'),
    confidence: jsonDouble(json, 'confidence'),
    targetExists: jsonBoolOrNull(json, 'targetExists'),
  );

  final String selectionId;
  final String path;
  final String name;
  final String code;
  final double confidence;
  final bool? targetExists;

  JsonMap toJson() => withoutNulls({
    'selectionId': selectionId,
    'path': path,
    'name': name,
    'code': code,
    'confidence': confidence,
    'targetExists': targetExists,
  });
}

/// A user decision on a review item (`ReviewDecision`).
final class ReviewDecision {
  const ReviewDecision({
    required this.itemId,
    required this.action,
    required this.code,
  });

  factory ReviewDecision.fromJson(JsonMap json) => ReviewDecision(
    itemId: jsonString(json, 'itemId'),
    action: ReviewDecisionAction.fromWire(jsonString(json, 'action')),
    code: jsonString(json, 'code'),
  );

  final String itemId;
  final ReviewDecisionAction action;
  final String code;

  JsonMap toJson() => {'itemId': itemId, 'action': action.name, 'code': code};
}
