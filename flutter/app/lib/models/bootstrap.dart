import 'auth.dart';
import 'json.dart';
import 'operation.dart';
import 'prefs.dart';

/// Protocol version this client understands; mirrors `Bootstrap.protocol`.
const int bootstrapProtocolVersion = 1;

/// Response of `app/bootstrap` (`Bootstrap`).
final class Bootstrap {
  const Bootstrap({
    required this.protocol,
    required this.version,
    required this.auth,
    required this.prefs,
    required this.operations,
  });

  factory Bootstrap.fromJson(JsonMap json) => Bootstrap(
    protocol: jsonInt(json, 'protocol'),
    version: jsonString(json, 'version'),
    auth: AuthState.fromJson(jsonObject(json, 'auth')),
    prefs: AppPrefs.fromJson(jsonObject(json, 'prefs')),
    operations: jsonObjects(json, 'operations', OperationDef.fromJson),
  );

  final int protocol;
  final String version;
  final AuthState auth;
  final AppPrefs prefs;
  final List<OperationDef> operations;

  JsonMap toJson() => {
    'protocol': protocol,
    'version': version,
    'auth': auth.toJson(),
    'prefs': prefs.toJson(),
    'operations': [for (final operation in operations) operation.toJson()],
  };
}
