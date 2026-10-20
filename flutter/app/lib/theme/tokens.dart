// Design tokens translated from frontend/src/styles.css.
//
// Light values are the `:root` block; dark values are the `:root[data-theme="dark"]`
// block, which the CSS also applies under `prefers-color-scheme: dark` for
// `data-theme="system"`. `--space` is the compact multiplier.

import 'package:flutter/material.dart';

/// Colors and layout tokens (`:root` custom properties).
@immutable
final class DocFlowTokens extends ThemeExtension<DocFlowTokens> {
  const DocFlowTokens({
    required this.bg,
    required this.surface,
    required this.surface2,
    required this.surface3,
    required this.text,
    required this.accent,
    required this.muted,
    required this.line,
    required this.lineStrong,
    required this.panelShadow,
    required this.inverse,
    required this.inverseText,
    required this.danger,
    required this.warning,
    required this.success,
    this.radius = 8,
    this.spaceScale = 1,
  });

  /// `:root` (light). `--panel-shadow` is `0 1px 0 rgb(34 36 35 / 0.04)`.
  static const DocFlowTokens light = DocFlowTokens(
    bg: Color(0xFFF4F5F3),
    surface: Color(0xFFFFFFFF),
    surface2: Color(0xFFE9EBE9),
    surface3: Color(0xFFD5D8D6),
    text: Color(0xFF222423),
    accent: Color(0xFF222423),
    muted: Color(0xFF626663),
    line: Color(0xFFD5D8D6),
    lineStrong: Color(0xFF727673),
    panelShadow: [BoxShadow(color: Color(0x0A222423), offset: Offset(0, 1))],
    inverse: Color(0xFF222423),
    inverseText: Color(0xFFFFFFFF),
    danger: Color(0xFF222423),
    warning: Color(0xFF626663),
    success: Color(0xFF222423),
  );

  /// `:root[data-theme="dark"]`. `--panel-shadow` is `none`.
  static const DocFlowTokens dark = DocFlowTokens(
    bg: Color(0xFF181A19),
    surface: Color(0xFF222423),
    surface2: Color(0xFF2B2E2C),
    surface3: Color(0xFF343835),
    text: Color(0xFFF4F5F3),
    accent: Color(0xFFF4F5F3),
    muted: Color(0xFFA4AAA6),
    line: Color(0xFF454A47),
    lineStrong: Color(0xFF727673),
    panelShadow: [],
    inverse: Color(0xFFF4F5F3),
    inverseText: Color(0xFF222423),
    danger: Color(0xFFF4F5F3),
    warning: Color(0xFFA4AAA6),
    success: Color(0xFFF4F5F3),
  );

  /// Palette for [brightness], with the compact spacing scale applied when
  /// [compact] is true (`:root[data-compact="true"] { --space: 0.78 }`).
  factory DocFlowTokens.forBrightness(
    Brightness brightness, {
    bool compact = false,
  }) {
    final base = brightness == Brightness.dark ? dark : light;
    return base.copyWith(spaceScale: compact ? compactSpaceScale : 1);
  }

  /// `--space` when `data-compact="true"`.
  static const double compactSpaceScale = 0.78;

  final Color bg;
  final Color surface;
  final Color surface2;
  final Color surface3;
  final Color text;
  final Color accent;
  final Color muted;
  final Color line;
  final Color lineStrong;
  final List<BoxShadow> panelShadow;
  final Color inverse;
  final Color inverseText;
  final Color danger;
  final Color warning;
  final Color success;

  /// `--radius`.
  final double radius;

  /// `--space`. CSS writes spacing as `calc(14px * var(--space))`; use [space].
  final double spaceScale;

  /// Scales a CSS pixel length by `--space`.
  double space(double pixels) => pixels * spaceScale;

  @override
  DocFlowTokens copyWith({
    Color? bg,
    Color? surface,
    Color? surface2,
    Color? surface3,
    Color? text,
    Color? accent,
    Color? muted,
    Color? line,
    Color? lineStrong,
    List<BoxShadow>? panelShadow,
    Color? inverse,
    Color? inverseText,
    Color? danger,
    Color? warning,
    Color? success,
    double? radius,
    double? spaceScale,
  }) => DocFlowTokens(
    bg: bg ?? this.bg,
    surface: surface ?? this.surface,
    surface2: surface2 ?? this.surface2,
    surface3: surface3 ?? this.surface3,
    text: text ?? this.text,
    accent: accent ?? this.accent,
    muted: muted ?? this.muted,
    line: line ?? this.line,
    lineStrong: lineStrong ?? this.lineStrong,
    panelShadow: panelShadow ?? this.panelShadow,
    inverse: inverse ?? this.inverse,
    inverseText: inverseText ?? this.inverseText,
    danger: danger ?? this.danger,
    warning: warning ?? this.warning,
    success: success ?? this.success,
    radius: radius ?? this.radius,
    spaceScale: spaceScale ?? this.spaceScale,
  );

  @override
  DocFlowTokens lerp(ThemeExtension<DocFlowTokens>? other, double t) {
    if (other is! DocFlowTokens) return this;
    return t < 0.5 ? this : other;
  }

  /// The tokens of the active theme, or the light palette when none is set.
  static DocFlowTokens of(BuildContext context) =>
      Theme.of(context).extension<DocFlowTokens>() ?? light;
}

/// Layout metrics of the app shell and common controls, in logical pixels.
/// Breakpoints are the CSS media query widths.
abstract final class DocFlowMetrics {
  DocFlowMetrics._();

  /// Side rail width above 1050px and between 820px and 1050px.
  static const double railWide = 232;
  static const double railMedium = 204;

  /// Icon-only rail between 620px and 820px.
  static const double railCompact = 68;

  /// Bottom navigation bar at 620px and below.
  static const double bottomBarHeight = 60;

  static const double breakpointRailMedium = 1050;
  static const double breakpointRailCompact = 820;
  static const double breakpointBottomBar = 620;

  static const double brandMark = 32;
  static const double brandMarkIcon = 18;
  static const double splashMark = 46;
  static const double navItemMinHeight = 44;
  static const double navItemCompactSize = 44;
  static const double navIcon = 18;
  static const double profileAvatar = 34;
  static const double emptyStateIconBox = 48;
  static const double emptyStateIcon = 22;
  static const double spinnerSize = 18;
  static const double emptyStateMinHeight = 230;
}
