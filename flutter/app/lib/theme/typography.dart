// Typography translated from the font rules in frontend/src/styles.css.

import 'package:flutter/material.dart';

import '../models/prefs.dart' show FontPreference;

/// The font stack for a user preference (`data-font`).
///
/// `system` leaves the family to the platform (the CSS stack is
/// `-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`). The other
/// choices name the same families the CSS names. Fallbacks follow the CSS
/// order.
// UNVERIFIED: family availability differs per desktop platform. Only Windows and
// macOS are supported, and their resolution of Georgia, Arial and Consolas is
// not checked on Linux.
({String? family, List<String> fallback}) fontStackFor(FontPreference font) =>
    switch (font) {
      FontPreference.system => (family: null, fallback: const <String>[]),
      FontPreference.sans => (
        family: 'Arial',
        fallback: const ['Helvetica', 'sans-serif'],
      ),
      FontPreference.serif => (
        family: 'Georgia',
        fallback: const ['Times New Roman', 'serif'],
      ),
      FontPreference.mono => (
        family: 'SFMono-Regular',
        fallback: const ['Consolas', 'monospace'],
      ),
    };

/// Sizes from styles.css, in rem (16px root). The root font size is
/// `fontScale * 16`, so every size scales with the preference.
abstract final class DocFlowTypeScale {
  DocFlowTypeScale._();

  /// Root font size in logical pixels at `fontScale = 1`.
  static const double rootPx = 16;

  /// `h1`: 1.75rem, letter-spacing -0.045em, line-height 1.15.
  static const double h1Rem = 1.75;
  static const double h1LetterSpacingEm = -0.045;
  static const double h1Height = 1.15;

  /// `h2`: 1.05rem, letter-spacing -0.015em.
  static const double h2Rem = 1.05;
  static const double h2LetterSpacingEm = -0.015;

  /// `.app-brand`: 1.0625rem (compact layout), letter-spacing -0.025em.
  static const double brandRem = 1.0625;
  static const double brandLetterSpacingEm = -0.025;

  /// Body text, the root size.
  static const double bodyRem = 1;

  /// `.app-nav-item`, `.app-profile-copy strong`: 0.875rem.
  static const double controlRem = 0.875;

  /// `.app-profile-copy small`: 0.8rem.
  static const double captionRem = 0.8;

  /// Converts rem to logical pixels at [fontScale].
  static double px(double rem, double fontScale) => rem * rootPx * fontScale;
}

/// Builds the Material text theme for a palette and preference.
TextTheme buildTextTheme({
  required Color color,
  required FontPreference font,
  required double fontScale,
}) {
  final stack = fontStackFor(font);
  double px(double rem) => DocFlowTypeScale.px(rem, fontScale);
  TextStyle style({
    required double rem,
    FontWeight weight = FontWeight.w400,
    double letterSpacingEm = 0,
    double? height,
  }) => TextStyle(
    fontFamily: stack.family,
    fontFamilyFallback: stack.fallback.isEmpty ? null : stack.fallback,
    fontSize: px(rem),
    fontWeight: weight,
    letterSpacing: letterSpacingEm * px(rem),
    height: height,
    color: color,
  );

  return TextTheme(
    headlineMedium: style(
      rem: DocFlowTypeScale.h1Rem,
      weight: FontWeight.w700,
      letterSpacingEm: DocFlowTypeScale.h1LetterSpacingEm,
      height: DocFlowTypeScale.h1Height,
    ),
    titleMedium: style(
      rem: DocFlowTypeScale.h2Rem,
      weight: FontWeight.w700,
      letterSpacingEm: DocFlowTypeScale.h2LetterSpacingEm,
    ),
    titleLarge: style(
      rem: DocFlowTypeScale.brandRem,
      weight: FontWeight.w700,
      letterSpacingEm: DocFlowTypeScale.brandLetterSpacingEm,
    ),
    bodyLarge: style(rem: DocFlowTypeScale.bodyRem),
    bodyMedium: style(rem: DocFlowTypeScale.controlRem),
    bodySmall: style(rem: DocFlowTypeScale.captionRem),
    // `.app-nav-item.active` uses font-weight 650, which Flutter cannot
    // express; w600 is the nearest step.
    labelLarge: style(
      rem: DocFlowTypeScale.controlRem,
      weight: FontWeight.w600,
    ),
  );
}
