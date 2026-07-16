import 'package:flutter/material.dart';

import '../../core/settings/app_prefs.dart';

/// Monokrom tema — renk yok, sadece gri tonlari.
///
/// ONEMLI KARAR: bu uygulama kullanicinin belgelerini TASIYOR. "Uyari" ile
/// "hata"yi ayirt etmek renk suslemesi degil, guvenlik gereksinimi. Monokromda
/// hue yok — o yuzden anlam IKON + KENARLIK + METIN ile tasiniyor, tona degil.
/// Yan fayda: renk korlugu olan kullanicilar icin de ayni derecede okunur.
/// Bir seyi "kirmiziya boyayarak" anlatma durtusune kapilirsan burayi oku.
abstract final class AppTheme {
  // Tek gri skalasi. Hepsi buradan turuyor — ekranlarda cıplak Color YOK.
  //
  // Her tonda R == G == B olmak ZORUNDA. "Neredeyse gri" paletler (or. Tailwind
  // zinc: 0xFF131316) hafif maviye calar ve monokrom degildir — bu tam olarak
  // bir kez basima geldi ve testi yazana kadar fark etmedim. Yeni ton eklerken
  // ucunu de esit tut; 'tema tamamen gri' testi aksini yakalar.
  static const _n0 = Color(0xFFFFFFFF);
  static const _n50 = Color(0xFFFAFAFA);
  static const _n100 = Color(0xFFF5F5F5);
  static const _n150 = Color(0xFFEDEDED);
  static const _n200 = Color(0xFFE5E5E5);
  static const _n300 = Color(0xFFD4D4D4);
  static const _n400 = Color(0xFFA3A3A3);
  static const _n500 = Color(0xFF737373);
  static const _n600 = Color(0xFF525252);
  static const _n700 = Color(0xFF404040);
  static const _n800 = Color(0xFF262626);
  static const _n850 = Color(0xFF1C1C1C);
  static const _n900 = Color(0xFF121212);
  static const _n950 = Color(0xFF0A0A0A);

  static ColorScheme _scheme(Brightness b) => b == Brightness.light
      ? const ColorScheme(
          brightness: Brightness.light,
          primary: _n900,
          onPrimary: _n0,
          primaryContainer: _n200,
          onPrimaryContainer: _n900,
          secondary: _n600,
          onSecondary: _n0,
          secondaryContainer: _n150,
          onSecondaryContainer: _n800,
          tertiary: _n500,
          onTertiary: _n0,
          // Monokromda "hata" da siyah — ayirt etme isini ikon/metin yapar.
          error: _n900,
          onError: _n0,
          errorContainer: _n200,
          onErrorContainer: _n900,
          surface: _n0,
          onSurface: _n900,
          surfaceContainerLowest: _n0,
          surfaceContainerLow: _n50,
          surfaceContainer: _n100,
          surfaceContainerHigh: _n150,
          surfaceContainerHighest: _n200,
          onSurfaceVariant: _n600,
          outline: _n300,
          outlineVariant: _n200,
          inverseSurface: _n900,
          onInverseSurface: _n50,
          inversePrimary: _n0,
          shadow: Color(0x14000000),
          scrim: Color(0x66000000),
        )
      : const ColorScheme(
          brightness: Brightness.dark,
          primary: _n50,
          onPrimary: _n900,
          primaryContainer: _n800,
          onPrimaryContainer: _n50,
          secondary: _n400,
          onSecondary: _n950,
          secondaryContainer: _n800,
          onSecondaryContainer: _n150,
          tertiary: _n500,
          onTertiary: _n950,
          error: _n50,
          onError: _n900,
          errorContainer: _n800,
          onErrorContainer: _n50,
          surface: _n950,
          onSurface: _n50,
          surfaceContainerLowest: _n950,
          surfaceContainerLow: _n900,
          surfaceContainer: _n850,
          surfaceContainerHigh: _n800,
          surfaceContainerHighest: _n700,
          onSurfaceVariant: _n400,
          outline: _n700,
          outlineVariant: _n800,
          inverseSurface: _n50,
          onInverseSurface: _n900,
          inversePrimary: _n900,
          shadow: Color(0x33000000),
          scrim: Color(0x99000000),
        );

  static ThemeData build({
    required Brightness brightness,
    required AppFont font,
    required double fontScale,
    required bool compact,
  }) {
    final cs = _scheme(brightness);
    final base = ThemeData(brightness: brightness, colorScheme: cs, useMaterial3: true);

    // Yazi tipi + boyut carpani tek noktadan; ekranlarda fontSize sabiti YOK.
    final text = base.textTheme.apply(
      fontFamily: font.family,
      fontFamilyFallback: font.fallback,
      fontSizeFactor: fontScale,
      bodyColor: cs.onSurface,
      displayColor: cs.onSurface,
    );

    return base.copyWith(
      textTheme: text,
      scaffoldBackgroundColor: cs.surface,
      visualDensity: compact ? VisualDensity.compact : VisualDensity.standard,
      splashFactory: InkSparkle.splashFactory,
      appBarTheme: AppBarTheme(
        backgroundColor: cs.surface,
        surfaceTintColor: Colors.transparent,
        foregroundColor: cs.onSurface,
        elevation: 0,
        scrolledUnderElevation: 0,
        centerTitle: false,
        titleTextStyle: text.titleMedium?.copyWith(fontWeight: FontWeight.w600),
        shape: Border(bottom: BorderSide(color: cs.outlineVariant)),
      ),
      cardTheme: CardThemeData(
        color: cs.surfaceContainerLowest,
        surfaceTintColor: Colors.transparent,
        elevation: 0,
        margin: EdgeInsets.zero,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(12),
          side: BorderSide(color: cs.outlineVariant),
        ),
      ),
      dividerTheme: DividerThemeData(color: cs.outlineVariant, thickness: 1, space: 1),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: cs.surfaceContainerLow,
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(10),
          borderSide: BorderSide(color: cs.outline),
        ),
        enabledBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(10),
          borderSide: BorderSide(color: cs.outline),
        ),
        focusedBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(10),
          borderSide: BorderSide(color: cs.primary, width: 2),
        ),
        contentPadding: const EdgeInsets.symmetric(horizontal: 14, vertical: 14),
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          minimumSize: const Size(0, 44), // dokunma/tiklama hedefi
          padding: const EdgeInsets.symmetric(horizontal: 20),
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
          textStyle: text.labelLarge?.copyWith(fontWeight: FontWeight.w600),
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
          minimumSize: const Size(0, 44),
          padding: const EdgeInsets.symmetric(horizontal: 20),
          side: BorderSide(color: cs.outline),
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
          textStyle: text.labelLarge,
        ),
      ),
      textButtonTheme: TextButtonThemeData(
        style: TextButton.styleFrom(
          minimumSize: const Size(0, 44),
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
          textStyle: text.labelLarge,
        ),
      ),
      chipTheme: ChipThemeData(
        backgroundColor: cs.surfaceContainer,
        side: BorderSide(color: cs.outlineVariant),
        labelStyle: text.labelSmall,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(6)),
      ),
      listTileTheme: ListTileThemeData(
        iconColor: cs.onSurfaceVariant,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
      ),
      snackBarTheme: SnackBarThemeData(
        backgroundColor: cs.inverseSurface,
        contentTextStyle: text.bodyMedium?.copyWith(color: cs.onInverseSurface),
        behavior: SnackBarBehavior.floating,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
      ),
      progressIndicatorTheme: ProgressIndicatorThemeData(
        color: cs.primary,
        linearTrackColor: cs.surfaceContainerHigh,
        circularTrackColor: cs.surfaceContainerHigh,
      ),
      switchTheme: SwitchThemeData(
        thumbColor: WidgetStateProperty.resolveWith(
            (s) => s.contains(WidgetState.selected) ? cs.onPrimary : cs.surface),
        trackColor: WidgetStateProperty.resolveWith(
            (s) => s.contains(WidgetState.selected) ? cs.primary : cs.surfaceContainerHigh),
        trackOutlineColor: WidgetStateProperty.all(cs.outline),
      ),
      tooltipTheme: TooltipThemeData(
        decoration: BoxDecoration(
          color: cs.inverseSurface,
          borderRadius: BorderRadius.circular(6),
        ),
        textStyle: text.bodySmall?.copyWith(color: cs.onInverseSurface),
      ),
    );
  }
}

/// Bir adimin/kaydin ciddiyeti. Renk TASIMAZ — ikon ve kenarlik tasir.
enum Severity { neutral, info, warn, danger, ok }

extension SeverityStyle on Severity {
  /// Anlami taşıyan sey bu: sekil. Monokromda tek ayirt edici.
  IconData get icon => switch (this) {
        Severity.neutral => Icons.remove,
        Severity.info => Icons.east,
        Severity.warn => Icons.error_outline, // uyari: ünlem, ici bos
        Severity.danger => Icons.report_gmailerrorred, // tehlike: dolu, agir
        Severity.ok => Icons.check,
      };

  /// Vurgu: tehlike/uyari daha kalin kenarlik + dolgu alir, sessizce kaybolmaz.
  bool get emphasized => this == Severity.warn || this == Severity.danger;
}
