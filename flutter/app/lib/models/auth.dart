import 'json.dart';

/// A signed-in account (`AppUser`).
final class AppUser {
  const AppUser({
    required this.id,
    required this.email,
    required this.username,
    required this.display,
    required this.allowed,
    required this.isAdmin,
    required this.createdAt,
  });

  factory AppUser.fromJson(JsonMap json) => AppUser(
    id: jsonString(json, 'id'),
    email: jsonString(json, 'email'),
    username: jsonStringOrNull(json, 'username'),
    display: jsonString(json, 'display'),
    allowed: jsonBool(json, 'allowed'),
    isAdmin: jsonBool(json, 'isAdmin'),
    createdAt: jsonString(json, 'createdAt'),
  );

  final String id;
  final String email;
  final String? username;
  final String display;
  final bool allowed;
  final bool isAdmin;
  final String createdAt;

  JsonMap toJson() => {
    'id': id,
    'email': email,
    'username': username,
    'display': display,
    'allowed': allowed,
    'isAdmin': isAdmin,
    'createdAt': createdAt,
  };
}

/// Session facts visible to the frontend (`AuthState.session`). `expiresAt` is
/// Unix seconds.
final class AuthSession {
  const AuthSession({
    required this.userId,
    required this.email,
    required this.expiresAt,
  });

  factory AuthSession.fromJson(JsonMap json) => AuthSession(
    userId: jsonString(json, 'userId'),
    email: jsonString(json, 'email'),
    expiresAt: jsonDouble(json, 'expiresAt'),
  );

  final String userId;
  final String email;
  final double expiresAt;

  JsonMap toJson() => {
    'userId': userId,
    'email': email,
    'expiresAt': expiresAt,
  };
}

/// Authentication state (`AuthState`).
final class AuthState {
  const AuthState({
    required this.signedIn,
    required this.allowed,
    required this.isAdmin,
    required this.session,
    required this.user,
  });

  /// Matches the signed-out state used before bootstrap and after sign-out.
  static const AuthState signedOut = AuthState(
    signedIn: false,
    allowed: false,
    isAdmin: false,
    session: null,
    user: null,
  );

  factory AuthState.fromJson(JsonMap json) => AuthState(
    signedIn: jsonBool(json, 'signedIn'),
    allowed: jsonBool(json, 'allowed'),
    isAdmin: jsonBool(json, 'isAdmin'),
    session: json['session'] == null
        ? null
        : AuthSession.fromJson(jsonObject(json, 'session')),
    user: json['user'] == null
        ? null
        : AppUser.fromJson(jsonObject(json, 'user')),
  );

  final bool signedIn;
  final bool allowed;
  final bool isAdmin;
  final AuthSession? session;
  final AppUser? user;

  JsonMap toJson() => {
    'signedIn': signedIn,
    'allowed': allowed,
    'isAdmin': isAdmin,
    'session': session?.toJson(),
    'user': user?.toJson(),
  };
}
