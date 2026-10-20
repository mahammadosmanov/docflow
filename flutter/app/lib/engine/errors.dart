/// Port of src-tauri/src/errors.rs.
///
/// Serializes to the same `{code, message}` protocol shape as the Rust
/// `BackendError` (serde `rename_all = "camelCase"`).
class BackendError implements Exception {
  const BackendError(this.code, this.message);

  /// Stable machine-readable code. Must match the Rust constant exactly.
  final String code;
  final String message;

  factory BackendError.protocol(String message) =>
      BackendError('backend_protocol_error', message);

  factory BackendError.stopped() => const BackendError(
    'backend_stopped',
    'DocFlow backend stopped unexpectedly',
  );

  factory BackendError.unavailable() => const BackendError(
    'backend_unavailable',
    'DocFlow backend could not be started',
  );

  factory BackendError.invalidParams() => const BackendError(
    'backend_invalid_params',
    'Backend parameters must be an object',
  );

  factory BackendError.requestFailed(String message) =>
      BackendError('backend_request_failed', message);

  factory BackendError.validation(String message) =>
      BackendError('validation', message);

  factory BackendError.offline() =>
      const BackendError('offline', 'network is unavailable');

  factory BackendError.networkTimeout() =>
      const BackendError('network_timeout', 'network request timed out');

  factory BackendError.responseTooLarge() => const BackendError(
    'network_response_too_large',
    'network response exceeded the size limit',
  );

  factory BackendError.authFailed(String message) =>
      BackendError('auth_failed', message);

  factory BackendError.authRequired() =>
      const BackendError('auth_required', 'not_authenticated');

  factory BackendError.permissionDenied() =>
      const BackendError('permission_denied', 'permission_denied');

  factory BackendError.credentialStore() => const BackendError(
    'credential_store_unavailable',
    'secure credential storage is unavailable',
  );

  factory BackendError.settingsWrite() => const BackendError(
    'settings_write_failed',
    'settings could not be saved',
  );

  factory BackendError.planUnavailable() => const BackendError(
    'plan_not_found_or_expired',
    'plan not found or expired; preview again',
  );

  factory BackendError.operationInProgress() => const BackendError(
    'operation_in_progress',
    'another operation is already running',
  );

  factory BackendError.cancelled() =>
      const BackendError('operation_cancelled', 'operation cancelled');

  factory BackendError.planOwnerMismatch() => const BackendError(
    'plan_owner_mismatch',
    'plan belongs to another owner',
  );

  factory BackendError.planSessionMismatch() => const BackendError(
    'plan_session_mismatch',
    'plan belongs to another session',
  );

  factory BackendError.pathGrantUnavailable() => const BackendError(
    'path_grant_not_found_or_expired',
    'path grant was not found or expired',
  );

  factory BackendError.pathGrantScopeMismatch() => const BackendError(
    'path_grant_scope_mismatch',
    'path grant belongs to another request or operation',
  );

  factory BackendError.pathTraversal() =>
      const BackendError('path_traversal', 'path traversal is not allowed');

  factory BackendError.pathEscape() =>
      const BackendError('path_escape', 'path escapes the granted root');

  factory BackendError.pathAccessDenied() => const BackendError(
    'path_access_denied',
    'path grant does not allow this access',
  );

  factory BackendError.pathInvalid(String message) =>
      BackendError('path_invalid', message);

  factory BackendError.journalOwnerMismatch() => const BackendError(
    'journal_owner_mismatch',
    'journal belongs to another owner',
  );

  factory BackendError.journalInvalid(String message) =>
      BackendError('journal_invalid', message);

  /// Same JSON shape as the Rust serde output: `{"code": .., "message": ..}`.
  Map<String, Object?> toJson() => {'code': code, 'message': message};

  @override
  bool operator ==(Object other) =>
      other is BackendError && other.code == code && other.message == message;

  @override
  int get hashCode => Object.hash(code, message);

  @override
  String toString() => 'BackendError($code: $message)';
}
