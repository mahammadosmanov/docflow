// Regenerates the localization inputs of this package from the frontend
// catalogs, which remain the source of truth for every user-visible string.
//
//   cd flutter/app && dart run lib/l10n/generate_catalog.dart
//
// Outputs:
// * lib/l10n/app_{az,en,ru,tr}.arb: gen-l10n inputs. gen-l10n only accepts
//   camelCase resource names, so each dotted key (`app.title`) becomes the
//   identifier `appTitle`. The mapping is deterministic and collision-checked.
// * lib/l10n/message_catalog.dart: the raw messages under their original dotted
//   keys. Runtime lookups (`Strings.t`) use these keys, because the engine and
//   the frontend refer to strings by dotted key (see frontend/src/i18n.ts).

import 'dart:convert';
import 'dart:io';

/// Locales in the order used by the frontend (`Locale` union in types.ts).
const List<String> catalogLocales = ['az', 'en', 'ru', 'tr'];

/// Locale whose ARB file defines the generated typed API.
const String templateLocale = 'en';

const Set<String> _dartReserved = {
  'abstract',
  'as',
  'assert',
  'async',
  'await',
  'break',
  'case',
  'catch',
  'class',
  'const',
  'continue',
  'covariant',
  'default',
  'deferred',
  'do',
  'dynamic',
  'else',
  'enum',
  'export',
  'extends',
  'extension',
  'external',
  'factory',
  'false',
  'final',
  'finally',
  'for',
  'Function',
  'get',
  'hide',
  'if',
  'implements',
  'import',
  'in',
  'interface',
  'is',
  'late',
  'library',
  'mixin',
  'new',
  'null',
  'on',
  'operator',
  'part',
  'required',
  'rethrow',
  'return',
  'set',
  'show',
  'static',
  'super',
  'switch',
  'sync',
  'this',
  'throw',
  'true',
  'try',
  'typedef',
  'var',
  'void',
  'while',
  'with',
  'yield',
};

final RegExp _dottedKey = RegExp(r'^[a-z][A-Za-z0-9]*([._][A-Za-z0-9]+)*$');

/// Maps a dotted catalog key to a gen-l10n resource name.
///
/// `op.rfi.process_scans.title` becomes `opRfiProcessScansTitle`.
String identifierForKey(String key) {
  if (!_dottedKey.hasMatch(key)) {
    throw FormatException('Unsupported catalog key: $key');
  }
  final parts = key.split(RegExp(r'[._]')).where((p) => p.isNotEmpty).toList();
  final buffer = StringBuffer(parts.first);
  for (final part in parts.skip(1)) {
    buffer
      ..write(part[0].toUpperCase())
      ..write(part.substring(1));
  }
  final identifier = buffer.toString();
  if (_dartReserved.contains(identifier)) {
    throw FormatException('Catalog key maps to reserved word: $key');
  }
  return identifier;
}

/// Dotted message keys of [arb], excluding ARB metadata entries (`@...`).
List<String> messageKeys(Map<String, Object?> arb) => [
  for (final key in arb.keys)
    if (!key.startsWith('@')) key,
];

/// Converts one locale's ARB map to gen-l10n form (camelCase resource names).
Map<String, Object?> toGenL10nArb(Map<String, Object?> arb, String locale) {
  final out = <String, Object?>{'@@locale': locale};
  final seen = <String, String>{};
  for (final entry in arb.entries) {
    if (entry.key == '@@locale') continue;
    final isMeta = entry.key.startsWith('@');
    final dotted = isMeta ? entry.key.substring(1) : entry.key;
    final identifier = identifierForKey(dotted);
    final previous = seen[identifier];
    if (previous != null && previous != dotted) {
      throw StateError('Identifier collision: $previous and $dotted');
    }
    seen[identifier] = dotted;
    out[isMeta ? '@$identifier' : identifier] = entry.value;
  }
  return out;
}

/// Renders the dotted-key catalog as a Dart source file.
String renderMessageCatalog(Map<String, Map<String, Object?>> arbByLocale) {
  final out = StringBuffer()
    ..writeln('// GENERATED FILE. Do not edit by hand.')
    ..writeln('// Source: tool/locales/app_*.arb')
    ..writeln(
      '// Regenerate: cd flutter/app && dart run lib/l10n/generate_catalog.dart',
    )
    ..writeln()
    ..writeln('/// Raw ARB messages keyed by dotted catalog keys, per locale.')
    ..writeln('///')
    ..writeln(
      '/// Placeholders stay in their `{name}` form and are substituted at',
    )
    ..writeln('/// lookup time, exactly as the original i18n loader did.')
    ..writeln('const Map<String, Map<String, String>> messageCatalog = {');
  for (final locale in catalogLocales) {
    out.writeln("  '$locale': {");
    final arb = arbByLocale[locale]!;
    for (final key in messageKeys(arb)) {
      final value = arb[key];
      if (value is! String) {
        throw FormatException('Non-string message $locale/$key');
      }
      out.writeln("    ${_dartString(key)}: ${_dartString(value)},");
    }
    out.writeln('  },');
  }
  out.writeln('};');
  return out.toString();
}

String _dartString(String value) {
  final escaped = value
      .replaceAll(r'\', r'\\')
      .replaceAll(r'$', r'\$')
      .replaceAll("'", r"\'")
      .replaceAll('\n', r'\n')
      .replaceAll('\r', r'\r');
  return "'$escaped'";
}

/// Reads the four frontend catalogs from [sourceDir].
Map<String, Map<String, Object?>> readCatalogs(String sourceDir) {
  final result = <String, Map<String, Object?>>{};
  for (final locale in catalogLocales) {
    final file = File('$sourceDir/app_$locale.arb');
    final decoded = jsonDecode(file.readAsStringSync());
    if (decoded is! Map<String, Object?>) {
      throw FormatException('ARB root is not an object: ${file.path}');
    }
    result[locale] = decoded;
  }
  _checkParity(result);
  return result;
}

void _checkParity(Map<String, Map<String, Object?>> byLocale) {
  final reference = messageKeys(byLocale[templateLocale]!).toSet();
  for (final locale in catalogLocales) {
    final keys = messageKeys(byLocale[locale]!).toSet();
    if (keys.length != reference.length || !keys.containsAll(reference)) {
      throw StateError('Catalog key sets differ for $locale');
    }
    for (final key in reference) {
      final expected = _placeholders(byLocale[templateLocale]![key] as String);
      final actual = _placeholders(byLocale[locale]![key] as String);
      if (expected.join(',') != actual.join(',')) {
        throw StateError('Placeholders differ for $locale/$key');
      }
    }
  }
}

List<String> _placeholders(String message) {
  final names = RegExp(
    r'\{([^{}]+)\}',
  ).allMatches(message).map((m) => m.group(1)!).toList()..sort();
  return names;
}

Future<void> main(List<String> args) async {
  final sourceDir = args.isEmpty ? 'tool/locales' : args.first;
  final outputDir = args.length > 1 ? args[1] : 'lib/l10n';
  final catalogs = readCatalogs(sourceDir);
  const encoder = JsonEncoder.withIndent('  ');
  final genArbs = <String, Map<String, Object?>>{};
  for (final locale in catalogLocales) {
    genArbs[locale] = toGenL10nArb(catalogs[locale]!, locale);
    File('$outputDir/app_$locale.arb')
        .writeAsStringSync('${encoder.convert(genArbs[locale])}\n');
  }
  File('$outputDir/message_catalog.dart')
      .writeAsStringSync(renderMessageCatalog(catalogs));
}
