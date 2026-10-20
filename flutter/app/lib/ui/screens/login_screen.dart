// LoginScreen, ported from `LoginScreen` in App.tsx (`.login-card`). Sign in
// with an email and password, or create an account with a username that is
// checked for availability after a 450ms pause.

import 'dart:async';

import 'package:flutter/material.dart' hide IconButton, Switch;

import '../../app/human_error.dart';
import '../../bridge/bridge.dart';
import '../../l10n/strings.dart';
import '../../models/auth.dart';
import '../../app/routes.dart';
import '../../app/screen_registry.dart';
import '../../theme/app_icons.dart';
import '../../theme/tokens.dart';
import '../widgets/metrics.dart';
import '../widgets/pressable.dart';
import '../widgets/spinner.dart';
import '../widgets/widgets.dart';
import 'screen_parts.dart';

/// Registers the login screen (the shell shows it when no one is signed in).
void registerLoginScreen(ScreenRegistry registry) {
  registry.register(
    ScreenId.login,
    (context) => LoginScreen(
      bridge: context.bridge,
      s: context.strings,
      onAuth: context.updateAuth,
      onSettings: () =>
          context.navigate(const SettingsRoute(back: SettingsBack.login)),
    ),
  );
}

/// Delay before a typed username is checked for availability.
const Duration usernameCheckDelay = Duration(milliseconds: 450);

/// A message under the form: an error, or the sign-up confirmation.
final class _LoginMessage {
  const _LoginMessage({required this.isError, required this.text});

  final bool isError;
  final String text;
}

class LoginScreen extends StatefulWidget {
  const LoginScreen({
    super.key,
    required this.bridge,
    required this.s,
    required this.onAuth,
    required this.onSettings,
  });

  final Bridge bridge;
  final Strings s;

  /// Receives the signed-in state after a successful sign-in.
  final ValueChanged<AuthState> onAuth;

  /// Opens the settings page (the gear in the header).
  final VoidCallback onSettings;

  @override
  State<LoginScreen> createState() => _LoginScreenState();
}

class _LoginScreenState extends State<LoginScreen> {
  bool _signUp = false;
  final _identifier = TextEditingController();
  final _email = TextEditingController();
  final _username = TextEditingController();
  final _password = TextEditingController();
  bool _showPassword = false;

  /// null until a username has been checked; true or false after.
  bool? _available;
  bool _checking = false;
  bool _busy = false;
  _LoginMessage? _message;

  Timer? _availabilityTimer;
  int _availabilityGeneration = 0;

  @override
  void dispose() {
    _availabilityTimer?.cancel();
    _identifier.dispose();
    _email.dispose();
    _username.dispose();
    _password.dispose();
    super.dispose();
  }

  Strings get _s => widget.s;

  /// Restarts the availability check for the current username. Any pending
  /// check or response from an earlier username is discarded.
  void _restartAvailability() {
    _availabilityTimer?.cancel();
    final generation = ++_availabilityGeneration;
    final name = _username.text;
    setState(() {
      _available = null;
      _checking = false;
    });
    if (!_signUp || !usernamePattern.hasMatch(name)) return;
    _availabilityTimer = Timer(usernameCheckDelay, () async {
      if (!mounted) return;
      setState(() => _checking = true);
      bool? available;
      try {
        available = await widget.bridge.call(
          FrontendCommands.authUsernameAvailable,
          params: (username: name),
        );
      } catch (_) {
        available = null;
      }
      if (!mounted || generation != _availabilityGeneration) return;
      setState(() {
        _available = available;
        _checking = false;
      });
    });
  }

  bool get _valid {
    if (_password.text.length < 6) return false;
    if (!_signUp) return _identifier.text.trim().contains('@');
    return _email.text.contains('@') &&
        usernamePattern.hasMatch(_username.text) &&
        _available != false;
  }

  void _setMode(bool signUp) {
    setState(() {
      _signUp = signUp;
      _message = null;
    });
    _restartAvailability();
  }

  Future<void> _submit() async {
    if (!_valid || _busy) return;
    setState(() {
      _busy = true;
      _message = null;
    });
    try {
      if (!_signUp) {
        final auth = await widget.bridge.call(
          FrontendCommands.authSignIn,
          params: (
            identifier: _identifier.text.trim(),
            password: _password.text,
          ),
        );
        widget.onAuth(auth);
      } else {
        await widget.bridge.call(
          FrontendCommands.authSignUp,
          params: (
            email: _email.text.trim(),
            username: _username.text.trim(),
            password: _password.text,
          ),
        );
        if (mounted) {
          setState(() {
            _message = _LoginMessage(
              isError: false,
              text: _s.t('auth.signupOk'),
            );
          });
        }
      }
    } catch (error) {
      if (mounted) {
        setState(() {
          _message = _LoginMessage(isError: true, text: humanError(error, _s));
        });
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final s = _s;
    final valid = _valid;
    final submitLabel = s.t(_signUp ? 'auth.signUp' : 'auth.signIn');

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Header(
          trailing: IconButton(
            label: s.t('prefs.title'),
            onPressed: widget.onSettings,
            child: Icon(AppIcons.settings),
          ),
        ),
        Expanded(
          child: LayoutBuilder(
            builder: (context, constraints) {
              final side = (constraints.maxWidth * 0.03).clamp(12.0, 32.0);
              final cardWidth = (constraints.maxWidth - side * 2).clamp(
                0.0,
                400.0,
              );
              return SingleChildScrollView(
                padding: EdgeInsets.symmetric(vertical: 16, horizontal: side),
                child: ConstrainedBox(
                  constraints: BoxConstraints(
                    minHeight: constraints.hasBoundedHeight
                        ? constraints.maxHeight - 32
                        : 0,
                  ),
                  child: Center(
                    child: SizedBox(
                      width: cardWidth,
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          const LargeBrandMark(),
                          Text(
                            s.t('app.title'),
                            textAlign: TextAlign.center,
                            style: Theme.of(context).textTheme.headlineMedium,
                          ),
                          const SizedBox(height: 4),
                          Text(
                            s.t(_signUp ? 'auth.subtitleUp' : 'auth.subtitle'),
                            textAlign: TextAlign.center,
                            style: TextStyle(color: t.muted),
                          ),
                          Padding(
                            padding: const EdgeInsets.only(top: 24, bottom: 20),
                            child: Segmented(
                              value: _signUp ? 'up' : 'in',
                              options: [
                                ('in', s.t('auth.signIn')),
                                ('up', s.t('auth.signUp')),
                              ],
                              onChanged: (value) => _setMode(value == 'up'),
                            ),
                          ),
                          _formStack(context),
                          if (_message != null)
                            Padding(
                              padding: const EdgeInsets.only(top: 14),
                              child: Notice(
                                kind: _message!.isError
                                    ? NoticeKind.error
                                    : NoticeKind.info,
                                child: Text(_message!.text),
                              ),
                            ),
                          DfButton(
                            wide: true,
                            onPressed: valid && !_busy ? _submit : null,
                            icon: _busy
                                ? Spinner(
                                    size: remPx(context, 1),
                                    color: t.inverseText,
                                  )
                                : Icon(AppIcons.arrowRight),
                            child: Text(submitLabel),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              );
            },
          ),
        ),
      ],
    );
  }

  Widget _formStack(BuildContext context) {
    final t = DocFlowTokens.of(context);
    final s = _s;
    final gap = SizedBox(height: 10 * t.spaceScale);

    final fields = <Widget>[];
    if (!_signUp) {
      fields.add(
        Field(
          autofocus: true,
          keyboardType: TextInputType.emailAddress,
          autofillHints: const [AutofillHints.email],
          label: s.t('auth.email'),
          help: s.t('auth.emailSignInHelp'),
          controller: _identifier,
          onChanged: (_) => setState(() {}),
          icon: Icon(AppIcons.user),
          onSubmitted: (_) => _submit(),
        ),
      );
    } else {
      fields
        ..add(
          Field(
            autofocus: true,
            keyboardType: TextInputType.emailAddress,
            autofillHints: const [AutofillHints.email],
            label: s.t('auth.email'),
            controller: _email,
            onChanged: (_) => setState(() {}),
            icon: Icon(AppIcons.user),
            onSubmitted: (_) => _submit(),
          ),
        )
        ..add(gap)
        ..add(
          Field(
            label: s.t('auth.username'),
            autofillHints: const [AutofillHints.username],
            help: _available == false
                ? s.t('auth.err.usernameTaken')
                : s.t('auth.usernameHelp'),
            invalid: _available == false,
            controller: _username,
            onChanged: (_) => _restartAvailability(),
            icon: Icon(AppIcons.badgeCheck),
            suffix: _checking
                ? Spinner(size: remPx(context, 1), color: t.muted)
                : _available == true
                ? Icon(AppIcons.check, size: remPx(context, 1), color: t.text)
                : null,
            onSubmitted: (_) => _submit(),
          ),
        );
    }
    fields
      ..add(gap)
      ..add(
        Field(
          obscureText: !_showPassword,
          autofillHints: [
            _signUp ? AutofillHints.newPassword : AutofillHints.password,
          ],
          label: s.t('auth.password'),
          help: s.t('auth.passwordHint'),
          controller: _password,
          onChanged: (_) => setState(() {}),
          icon: Icon(AppIcons.shield),
          textInputAction: TextInputAction.done,
          suffix: _FieldAction(
            label: s.t(_showPassword ? 'auth.hide' : 'auth.show'),
            icon: _showPassword ? AppIcons.eyeOff : AppIcons.eye,
            onPressed: () => setState(() => _showPassword = !_showPassword),
          ),
          onSubmitted: (_) => _submit(),
        ),
      );

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: fields,
    );
  }
}

/// The `.field-action` button at the end of an input: a 30px square with an
/// icon and a tooltip that doubles as its accessible name.
class _FieldAction extends StatelessWidget {
  const _FieldAction({
    required this.label,
    required this.icon,
    required this.onPressed,
  });

  final String label;
  final IconData icon;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final t = DocFlowTokens.of(context);
    return Tooltip(
      message: label,
      child: Pressable(
        label: label,
        onPressed: onPressed,
        builder: (context, hovered, focused) => FocusRing(
          visible: focused,
          radius: 7,
          child: Container(
            width: 30,
            height: 30,
            alignment: Alignment.center,
            decoration: BoxDecoration(
              color: hovered ? t.surface3 : Colors.transparent,
              borderRadius: BorderRadius.circular(7),
            ),
            child: Icon(icon, size: remPx(context, 1), color: t.text),
          ),
        ),
      ),
    );
  }
}
