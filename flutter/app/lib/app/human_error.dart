// Port of `humanError` and `isDuplicatePresetError` from frontend/src/App.tsx.
// Engine errors carry localization keys (`error.preset.duplicate`) or stable
// codes (`invalid_credentials`). These are mapped to the user's language.

import '../bridge/bridge.dart';
import '../l10n/strings.dart';

RegExp _ci(String pattern) => RegExp(pattern, caseSensitive: false);

final RegExp _localizedKey = RegExp(r'^(?:Error:\s*)?(error\.[A-Za-z0-9_.]+)$');
final RegExp _detailedKey = RegExp(
  r'^(?:Error:\s*)?(error\.[A-Za-z0-9_.]+):\s*(.+)$',
);
final RegExp _presetNameInvalid = _ci(
  r'error\.preset\.nameInvalid|preset[_ .-]?name[_ .-]?invalid',
);
final RegExp _presetDuplicate = _ci(
  r'error\.preset\.duplicate|duplicate[_ .-]?preset|preset.*already exists',
);
final RegExp _presetLimit = _ci(
  r'error\.preset\.limit|preset[_ .-]?(?:capacity|limit)',
);
final RegExp _presetStale = _ci(
  r'error\.preset\.stale|preset[_ .-]?(?:missing|stale|not found)',
);
final RegExp _presetWrite = _ci(
  r'error\.preset\.write|preset.*(?:write|persist|save).*fail',
);
final RegExp _workbookInvalid = RegExp(
  r'^(?:Error:\s*)?error\.rfi\.workbookInvalid$',
);
final RegExp _queueReselection = RegExp(
  r'^(?:Error:\s*)?error\.print\.queueSourceReselectionRequired$',
);
final RegExp _emailRequired = _ci(r'email_required');
final RegExp _invalidCredentials = _ci(r'invalid_credentials|Invalid login');
final RegExp _emailNotConfirmed = _ci(
  r'email_not_confirmed|Email not confirmed',
);
final RegExp _rateLimit = _ci(r'rate.limit|too many requests');
final RegExp _usernameTaken = _ci(r'username_taken');
final RegExp _usernameInvalid = _ci(r'username_invalid');
final RegExp _emailTaken = _ci(r'User already registered|user_already_exists');
final RegExp _weakPassword = _ci(r'weak_password|Password should be');
final RegExp _errorPrefix = RegExp(r'^Error:\s*');

/// Turns an engine or bridge failure into a message for the user.
///
/// A localized key with a translation is rendered in the active language. A
/// key with details appends them in parentheses. Otherwise the raw message is
/// returned, without a leading `Error:`.
String humanError(Object error, Strings s) {
  final raw = asBridgeError(error).message;

  final localizedKey = _localizedKey.firstMatch(raw)?.group(1);
  if (localizedKey != null) {
    final localized = s.t(localizedKey);
    if (localized != localizedKey) return localized;
  }

  final detailed = _detailedKey.firstMatch(raw);
  if (detailed != null) {
    final key = detailed.group(1)!;
    final details = detailed.group(2)!;
    final localized = s.t(key);
    if (localized != key) return '$localized ($details)';
  }

  if (_presetNameInvalid.hasMatch(raw)) return s.t('error.preset.nameInvalid');
  if (_presetDuplicate.hasMatch(raw)) return s.t('error.preset.duplicate');
  if (_presetLimit.hasMatch(raw)) return s.t('error.preset.limit');
  if (_presetStale.hasMatch(raw)) return s.t('error.preset.stale');
  if (_presetWrite.hasMatch(raw)) return s.t('error.preset.write');
  if (_workbookInvalid.hasMatch(raw)) return s.t('error.rfi.workbookInvalid');
  if (_queueReselection.hasMatch(raw)) {
    return s.t('error.print.queueSourceReselectionRequired');
  }
  if (_emailRequired.hasMatch(raw)) return s.t('auth.emailSignInHelp');
  if (_invalidCredentials.hasMatch(raw)) return s.t('auth.err.credentials');
  if (_emailNotConfirmed.hasMatch(raw)) {
    return s.t('auth.err.emailNotConfirmed');
  }
  if (_rateLimit.hasMatch(raw)) return s.t('auth.err.rateLimit');
  if (_usernameTaken.hasMatch(raw)) return s.t('auth.err.usernameTaken');
  if (_usernameInvalid.hasMatch(raw)) return s.t('auth.err.usernameInvalid');
  if (_emailTaken.hasMatch(raw)) return s.t('auth.err.emailTaken');
  if (_weakPassword.hasMatch(raw)) return s.t('auth.err.weakPassword');
  return raw.replaceFirst(_errorPrefix, '');
}

/// True when [error] reports a duplicate preset name.
bool isDuplicatePresetError(Object error) {
  final raw = asBridgeError(error).message;
  return _presetDuplicate.hasMatch(raw);
}
