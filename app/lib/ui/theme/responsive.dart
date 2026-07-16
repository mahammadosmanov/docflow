import 'package:flutter/material.dart';

/// Pencere genisligi siniflari.
///
/// "Her PC'de calissin" pratikte sudur: 1366x768 ofis dizustuyle 4K masaustu
/// ayni kodu paylasir. Ekranlarda cıplak genislik sabiti YOK — hepsi buradan.
enum FormFactor {
  /// Dar pencere / bolunmus ekran: tek sutun, ikon-only aksiyonlar.
  compact,

  /// Tipik dizustu.
  medium,

  /// Genis masaustu.
  expanded;

  static FormFactor of(BuildContext context) {
    final w = MediaQuery.sizeOf(context).width;
    if (w < 640) return FormFactor.compact;
    if (w < 1080) return FormFactor.medium;
    return FormFactor.expanded;
  }

  bool get isCompact => this == FormFactor.compact;

  /// Kenar boslugu: dar pencerede yer israf etme.
  double get gutter => switch (this) {
        FormFactor.compact => 16,
        FormFactor.medium => 24,
        FormFactor.expanded => 32,
      };
}

/// Icerigi ortalar, okunabilir bir satir uzunlugunda tutar ve kenar boslugunu
/// pencereye gore ayarlar. Tam ekranda metin bastan basa uzamaz.
class Pane extends StatelessWidget {
  const Pane({
    super.key,
    required this.child,
    this.maxWidth = 720,
    this.padded = true,
  });

  final Widget child;
  final double maxWidth;
  final bool padded;

  @override
  Widget build(BuildContext context) {
    final ff = FormFactor.of(context);
    return Align(
      alignment: Alignment.topCenter,
      // heightFactor 1.0 sart: Pane hem ListView icinde (yukseklik SINIRSIZ —
      // Align burada sinirsiza yayilmaya calisip patlar) hem de dogrudan
      // Scaffold.body'de (SINIRLI — Expanded'li Column'un dolmasi gerek)
      // kullaniliyor. Bu ikisini ayni anda calistiran tek ayar bu.
      heightFactor: 1.0,
      child: ConstrainedBox(
        constraints: BoxConstraints(maxWidth: maxWidth),
        child: Padding(
          padding: padded
              ? EdgeInsets.symmetric(horizontal: ff.gutter, vertical: ff.gutter)
              : EdgeInsets.zero,
          child: child,
        ),
      ),
    );
  }
}

/// Baslik + aciklama + govde olan bolum. Ayarlar/adminde tekrar eden kalip.
class Section extends StatelessWidget {
  const Section({super.key, required this.title, required this.child, this.subtitle});

  final String title;
  final String? subtitle;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.only(left: 4, bottom: 8),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(title,
                  style: t.textTheme.labelMedium?.copyWith(
                    color: t.colorScheme.onSurfaceVariant,
                    fontWeight: FontWeight.w600,
                    letterSpacing: 0.6,
                  )),
              if (subtitle != null)
                Padding(
                  padding: const EdgeInsets.only(top: 2),
                  child: Text(subtitle!,
                      style: t.textTheme.bodySmall
                          ?.copyWith(color: t.colorScheme.onSurfaceVariant)),
                ),
            ],
          ),
        ),
        Card(child: child),
        const SizedBox(height: 24),
      ],
    );
  }
}

/// Bos/hata durumu — ikon + baslik + aciklama + (opsiyonel) aksiyon.
/// Bos ekrani asla cıplak birakma: kullanici "bozuldu mu?" diye dusunmesin.
class EmptyState extends StatelessWidget {
  const EmptyState({
    super.key,
    required this.icon,
    required this.title,
    this.message,
    this.action,
  });

  final IconData icon;
  final String title;
  final String? message;
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    final t = Theme.of(context);
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 40, color: t.colorScheme.onSurfaceVariant),
            const SizedBox(height: 14),
            Text(title,
                textAlign: TextAlign.center,
                style: t.textTheme.titleMedium?.copyWith(fontWeight: FontWeight.w600)),
            if (message != null) ...[
              const SizedBox(height: 6),
              Text(message!,
                  textAlign: TextAlign.center,
                  style: t.textTheme.bodyMedium
                      ?.copyWith(color: t.colorScheme.onSurfaceVariant)),
            ],
            if (action != null) ...[const SizedBox(height: 20), action!],
          ],
        ),
      ),
    );
  }
}
