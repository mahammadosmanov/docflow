// Builds the Material theme from the DocFlow tokens. The React app set
// `data-theme`, `data-font`, `data-compact` and the root font size on <html>
// (App.tsx applyAppearance). Here those become ThemeData parameters.

import 'package:flutter/material.dart';

import '../models/prefs.dart';
import 'tokens.dart';
import 'typography.dart';

/// Theme preference to Material [ThemeMode] (`data-theme`).
ThemeMode themeModeFor(ThemePreference preference) => switch (preference) {
  ThemePreference.system => ThemeMode.system,
  ThemePreference.light => ThemeMode.light,
  ThemePreference.dark => ThemeMode.dark,
};

/// The Material theme for [brightness] under the given appearance preferences.
ThemeData buildAppTheme({
  required Brightness brightness,
  FontPreference font = FontPreference.system,
  double fontScale = 1,
  bool compact = false,
}) {
  final tokens = DocFlowTokens.forBrightness(brightness, compact: compact);
  final scheme = ColorScheme(
    brightness: brightness,
    primary: tokens.accent,
    onPrimary: tokens.inverseText,
    secondary: tokens.muted,
    onSecondary: tokens.bg,
    error: tokens.danger,
    onError: tokens.surface,
    surface: tokens.surface,
    onSurface: tokens.text,
    surfaceContainerLowest: tokens.bg,
    surfaceContainerLow: tokens.surface,
    surfaceContainer: tokens.surface2,
    surfaceContainerHigh: tokens.surface2,
    surfaceContainerHighest: tokens.surface3,
    outline: tokens.lineStrong,
    outlineVariant: tokens.line,
  );
  final stack = fontStackFor(font);
  return ThemeData(
    useMaterial3: true,
    brightness: brightness,
    colorScheme: scheme,
    scaffoldBackgroundColor: tokens.bg,
    canvasColor: tokens.surface,
    dividerColor: tokens.line,
    fontFamily: stack.family,
    fontFamilyFallback: stack.fallback.isEmpty ? null : stack.fallback,
    textTheme: buildTextTheme(
      color: tokens.text,
      font: font,
      fontScale: fontScale,
    ),
    iconTheme: IconThemeData(color: tokens.text, size: 17.6),
    extensions: [tokens],
  );
}
