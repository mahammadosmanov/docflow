// Saves a diagnostics bundle the user asked to download (the Download button
// of the support group in the React SettingsScreen, which used a browser Blob).
//
// UNVERIFIED: the engine has no save-file command yet (AGENTS.md says the Rust
// engine owns filesystem writes). Until `support/saveDiagnostics` exists, the
// default [FileDiagnosticsSaver] writes from the UI layer into the user's
// Downloads folder. It never overwrites: a name that is taken gets " (n)". This
// must move behind the engine, and the default replaced, before release.

import 'dart:io';

import 'package:path/path.dart' as p;

import '../../models/support.dart';

/// Writes a diagnostics bundle to a location the user can find, and reports
/// the path it was written to.
abstract interface class DiagnosticsSaver {
  Future<String> save(SupportBundlePreview bundle);
}

/// The default saver: writes the bundle into [directory], or the user's
/// Downloads folder when none is given.
final class FileDiagnosticsSaver implements DiagnosticsSaver {
  const FileDiagnosticsSaver({this.directory});

  /// Destination folder. Null means the Downloads folder of the user.
  final String? directory;

  @override
  Future<String> save(SupportBundlePreview bundle) async {
    final dir = Directory(directory ?? _downloadsDirectory());
    await dir.create(recursive: true);
    // Keep only the base name, so the engine-provided name cannot leave [dir].
    final fileName = p.basename(bundle.fileName);
    final stem = p.basenameWithoutExtension(fileName);
    final extension = p.extension(fileName);
    for (var n = 0; ; n++) {
      final name = n == 0 ? fileName : '$stem ($n)$extension';
      final file = File(p.join(dir.path, name));
      try {
        // exclusive: fails when the name is taken, so nothing is overwritten.
        await file.create(exclusive: true);
      } on FileSystemException {
        continue;
      }
      await file.writeAsString(bundle.content, flush: true);
      return file.path;
    }
  }
}

String _downloadsDirectory() {
  final env = Platform.environment;
  final home = env['USERPROFILE'] ?? env['HOME'] ?? Directory.current.path;
  return p.join(home, 'Downloads');
}
