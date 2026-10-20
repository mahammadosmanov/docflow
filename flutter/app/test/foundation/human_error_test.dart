import 'package:docflow/app/app_utils.dart';
import 'package:docflow/app/human_error.dart';
import 'package:docflow/bridge/bridge.dart';
import 'package:docflow/l10n/strings.dart';
import 'package:docflow/models/models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final en = makeStrings(AppLocale.en);

  String translate(Object error) => humanError(error, en);

  test('a localized key renders in the active language', () {
    expect(
      translate(const BridgeError(message: 'error.preset.duplicate')),
      en.t('error.preset.duplicate'),
    );
  });

  test('a localized key with details appends them in parentheses', () {
    expect(
      translate(const BridgeError(message: 'Error: error.preset.limit: 12/10')),
      '${en.t('error.preset.limit')} (12/10)',
    );
    expect(
      translate(
        const BridgeError(message: 'error.preset.nameInvalid: bad name'),
      ),
      '${en.t('error.preset.nameInvalid')} (bad name)',
    );
  });

  test('auth error codes map to their messages', () {
    expect(
      translate(const BridgeError(message: 'invalid_credentials')),
      en.t('auth.err.credentials'),
    );
    expect(
      translate(const BridgeError(message: 'User already registered')),
      en.t('auth.err.emailTaken'),
    );
    expect(
      translate(const BridgeError(message: 'Too many requests, rate limit')),
      en.t('auth.err.rateLimit'),
    );
  });

  test('unknown messages drop a leading Error: prefix', () {
    expect(
      translate(const BridgeError(message: 'Error: disk full')),
      'disk full',
    );
    expect(translate('plain failure'), 'plain failure');
  });

  test('isDuplicatePresetError recognizes duplicate names', () {
    expect(isDuplicatePresetError('preset "x" already exists'), isTrue);
    expect(isDuplicatePresetError('disk full'), isFalse);
  });

  test('parseRfiInputs splits, trims and removes duplicates', () {
    expect(parseRfiInputs(' 12, 13;12\n14  '), ['12', '13', '14']);
    expect(parseRfiInputs(null), isEmpty);
  });

  test('asPathSelection requires selectionId and displayPath', () {
    expect(
      asPathSelection({'selectionId': 's', 'displayPath': 'd', 'name': 'n'})
          ?.name,
      'n',
    );
    expect(asPathSelection({'selectionId': 's'}), isNull);
  });

  test('usernamePattern accepts 3 to 24 safe characters', () {
    expect(usernamePattern.hasMatch('ada.l_1'), isTrue);
    expect(usernamePattern.hasMatch('ab'), isFalse);
    expect(usernamePattern.hasMatch('a' * 25), isFalse);
    expect(usernamePattern.hasMatch('a b'), isFalse);
  });
}
