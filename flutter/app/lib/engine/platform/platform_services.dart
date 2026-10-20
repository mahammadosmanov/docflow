// Production PlatformServices: the OS seams the Tauri shell provided through
// tauri-plugin-dialog, tauri-plugin-opener, clipboard.rs, platform/startup.rs
// and ocr.rs. Dialogs run the OS picker in a child process so no plugin is
// needed: AppleScript `choose` on macOS, a WinForms dialog through PowerShell
// on Windows, and zenity on Linux.

import 'dart:convert';
import 'dart:io';

import '../backend.dart';
import '../clipboard.dart';
import '../errors.dart';
import '../ocr.dart';
import 'startup.dart' as startup;

final class ProductionPlatformServices implements PlatformServices {
  ProductionPlatformServices({NativeOcr? ocr}) : _ocr = ocr;

  NativeOcr? _ocr;

  @override
  Future<List<String>> pickPaths({
    required PathSelectionMode mode,
    DialogFilter? filter,
  }) async {
    final ProcessResult result;
    try {
      result = await _runPicker(mode, filter);
    } on ProcessException {
      throw BackendError.requestFailed('path selection task failed');
    }
    // Every picker reports a cancelled dialog as a non-zero exit.
    if (result.exitCode != 0) return const [];
    return const LineSplitter()
        .convert('${result.stdout}')
        .map((line) => line.trim())
        .where((line) => line.isNotEmpty)
        .map(_stripTrailingSeparator)
        .toList();
  }

  Future<ProcessResult> _runPicker(
    PathSelectionMode mode,
    DialogFilter? filter,
  ) {
    if (Platform.isMacOS) {
      return Process.run('osascript', ['-e', _appleScript(mode, filter)]);
    }
    if (Platform.isWindows) {
      // UNVERIFIED: WinForms dialog through PowerShell is not exercised on a
      // Windows host in this port.
      return Process.run('powershell.exe', [
        '-NoProfile',
        '-STA',
        '-Command',
        _powerShell(mode, filter),
      ]);
    }
    return Process.run('zenity', [
      '--file-selection',
      if (mode == PathSelectionMode.folder) '--directory',
      if (mode == PathSelectionMode.files) ...['--multiple', '--separator=\n'],
      if (filter != null)
        '--file-filter=${filter.name} | '
            '${filter.extensions.map((e) => '*.$e').join(' ')}',
    ]);
  }

  @override
  Future<void> openPath(String path) async {
    final (command, arguments) = Platform.isMacOS
        ? ('open', [path])
        : Platform.isWindows
        ? ('explorer.exe', [path])
        : ('xdg-open', [path]);
    try {
      final result = await Process.run(command, arguments);
      // explorer.exe exits 1 even when it opened the path.
      if (result.exitCode != 0 && !Platform.isWindows) {
        throw BackendError.requestFailed('selected path could not be opened');
      }
    } on ProcessException {
      throw BackendError.requestFailed('selected path could not be opened');
    }
  }

  @override
  Future<void> writeClipboard(String content) =>
      clipboardWritePlain(content, PlatformClipboardAdapter());

  @override
  void setStartWithWindows(bool enabled) {
    try {
      startup.setStartWithWindows(enabled: enabled);
    } on startup.StartupException {
      throw BackendError.settingsWrite();
    }
  }

  @override
  ({bool available, bool languageData}) ocrSupport() {
    final (available, languageData) = (_ocr ??= NativeOcr.discover())
        .supportStatus();
    return (available: available, languageData: languageData);
  }
}

String _stripTrailingSeparator(String path) =>
    path.length > 1 && (path.endsWith('/') || path.endsWith(r'\'))
    ? path.substring(0, path.length - 1)
    : path;

String _appleString(String value) =>
    '"${value.replaceAll(r'\', r'\\').replaceAll('"', r'\"')}"';

String _appleScript(PathSelectionMode mode, DialogFilter? filter) {
  final types = filter == null || filter.extensions.isEmpty
      ? ''
      : ' of type {${filter.extensions.map(_appleString).join(', ')}}';
  return switch (mode) {
    PathSelectionMode.folder => 'POSIX path of (choose folder)',
    PathSelectionMode.file => 'POSIX path of (choose file$types)',
    PathSelectionMode.files =>
      'set picked to choose file$types with multiple selections allowed\n'
          'set out to ""\n'
          'repeat with f in picked\n'
          'set out to out & POSIX path of f & linefeed\n'
          'end repeat\n'
          'return out',
  };
}

String _powerShellString(String value) => "'${value.replaceAll("'", "''")}'";

String _powerShell(PathSelectionMode mode, DialogFilter? filter) {
  const prelude = 'Add-Type -AssemblyName System.Windows.Forms;';
  if (mode == PathSelectionMode.folder) {
    return '$prelude\$d = New-Object System.Windows.Forms.FolderBrowserDialog;'
        "if (\$d.ShowDialog() -eq 'OK') { \$d.SelectedPath } else { exit 1 }";
  }
  final pattern = filter == null || filter.extensions.isEmpty
      ? ''
      : '\$d.Filter = ${_powerShellString('${filter.name}|${filter.extensions.map((e) => '*.$e').join(';')}')};';
  final multi = mode == PathSelectionMode.files ? r'$true' : r'$false';
  return '$prelude\$d = New-Object System.Windows.Forms.OpenFileDialog;'
      '$pattern\$d.Multiselect = $multi;'
      "if (\$d.ShowDialog() -eq 'OK') { \$d.FileNames } else { exit 1 }";
}
