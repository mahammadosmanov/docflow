import 'package:docflow/models/models.dart';
import 'package:docflow/theme/app_theme.dart';
import 'package:docflow/theme/tokens.dart';
import 'package:docflow/theme/typography.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('light and dark palettes match the :root blocks in styles.css', () {
    final light = DocFlowTokens.light;
    expect(light.bg, const Color(0xFFF4F5F3));
    expect(light.surface, const Color(0xFFFFFFFF));
    expect(light.text, const Color(0xFF222423));
    expect(light.muted, const Color(0xFF626663));
    expect(light.line, const Color(0xFFD5D8D6));
    expect(light.inverseText, const Color(0xFFFFFFFF));
    expect(light.radius, 8);
    expect(light.panelShadow, hasLength(1));

    final dark = DocFlowTokens.dark;
    expect(dark.bg, const Color(0xFF181A19));
    expect(dark.surface2, const Color(0xFF2B2E2C));
    expect(dark.muted, const Color(0xFFA4AAA6));
    expect(dark.line, const Color(0xFF454A47));
    expect(dark.inverse, const Color(0xFFF4F5F3));
    expect(dark.panelShadow, isEmpty);
  });

  test('compact mode scales spacing by 0.78 as data-compact does', () {
    final tokens = DocFlowTokens.forBrightness(Brightness.light, compact: true);
    expect(tokens.space(14), closeTo(10.92, 1e-9));
    expect(DocFlowTokens.forBrightness(Brightness.dark).space(14), 14);
  });

  test('font scale applies to type sizes: rem * 16 * fontScale', () {
    expect(DocFlowTypeScale.px(DocFlowTypeScale.h1Rem, 1), 28);
    expect(DocFlowTypeScale.px(DocFlowTypeScale.controlRem, 1.25), 17.5);
  });

  test('each font preference maps to its CSS family stack', () {
    expect(fontStackFor(FontPreference.system).family, isNull);
    expect(fontStackFor(FontPreference.sans).family, 'Arial');
    expect(fontStackFor(FontPreference.serif).family, 'Georgia');
    expect(fontStackFor(FontPreference.mono).family, 'SFMono-Regular');
  });

  test('the theme carries the palette and text scale', () {
    final theme = buildAppTheme(brightness: Brightness.dark, fontScale: 1.5);
    expect(theme.brightness, Brightness.dark);
    expect(theme.scaffoldBackgroundColor, DocFlowTokens.dark.bg);
    expect(theme.textTheme.bodyLarge?.fontSize, 24);
    expect(theme.extension<DocFlowTokens>(), isNotNull);
  });

  test('theme preference maps to ThemeMode', () {
    expect(themeModeFor(ThemePreference.system), ThemeMode.system);
    expect(themeModeFor(ThemePreference.light), ThemeMode.light);
    expect(themeModeFor(ThemePreference.dark), ThemeMode.dark);
  });
}
