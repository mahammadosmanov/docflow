# i18n notu

`flutter gen-l10n` bu ARB dosyalariyla CALISMAZ: anahtarlar noktali
("app.title", "op.rfi.process_scans.title") ve gen-l10n bunlari gecerli Dart
metod adi olarak reddeder ("Invalid ARB resource name"). Ayrica sidecar
anahtarlari RUNTIME'da string olarak yolluyor; gen-l10n'in statik getter'lari
bunu zaten cozemez.

Bu yuzden i18n runtime'da yapiliyor: dort ARB asset olarak yuklenip
`lib/core/l10n/strings.dart` icinde anahtar->metin cozuluyor. Kural 4 korunuyor
(her string bir ARB anahtari, dort dilde de var).

`l10n.yaml` -> `l10n.yaml.disabled` olarak devre disi: varligi macOS build'inde
gen_localizations adimini tetikleyip derlemeyi kiriyordu.
Karar acik: ya ARB anahtarlarini camelCase'e cevir + gen-l10n'e don, ya da bu
runtime yaklasimi resmilestir.
