import 'json.dart';

enum ReplicaAutomationState {
  unconfigured('unconfigured'),
  disabled('disabled'),
  idle('idle'),
  running('running'),
  waitingForSignIn('waitingForSignIn'),
  deferred('deferred'),
  paused('paused');

  const ReplicaAutomationState(this.wire);

  final String wire;
}

/// Replica publishing automation status (`ReplicaAutomationStatus`).
final class ReplicaAutomationStatus {
  const ReplicaAutomationStatus({
    required this.enabled,
    required this.configured,
    required this.status,
    required this.lastSuccessUnix,
    required this.lastErrorCode,
  });

  factory ReplicaAutomationStatus.fromJson(JsonMap json) =>
      ReplicaAutomationStatus(
        enabled: jsonBool(json, 'enabled'),
        configured: jsonBool(json, 'configured'),
        status: jsonEnum(
          json,
          'status',
          ReplicaAutomationState.values,
          (value) => value.wire,
        ),
        lastSuccessUnix: jsonIntOrNull(json, 'lastSuccessUnix'),
        lastErrorCode: jsonStringOrNull(json, 'lastErrorCode'),
      );

  final bool enabled;
  final bool configured;
  final ReplicaAutomationState status;
  final int? lastSuccessUnix;
  final String? lastErrorCode;

  JsonMap toJson() => {
    'enabled': enabled,
    'configured': configured,
    'status': status.wire,
    'lastSuccessUnix': lastSuccessUnix,
    'lastErrorCode': lastErrorCode,
  };
}
