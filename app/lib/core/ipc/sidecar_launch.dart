import 'dart:io';

class SidecarLaunch {
  const SidecarLaunch(
    this.executable,
    this.args,
    this.workingDir, [
    this.environment,
  ]);

  final String executable;
  final List<String> args;
  final String workingDir;
  final Map<String, String>? environment;
}

/// Resolves the packaged sidecar first, then development overrides/fallbacks.
///
/// The supported package layout is flat (`sidecar/docflow_sidecar.exe`).
/// The nested PyInstaller layout is also accepted defensively so an otherwise
/// complete package cannot strand the UI on the background-service error page.
SidecarLaunch resolveSidecarLaunch({
  required String devPython,
  required String devSidecarDir,
  String? resolvedExecutable,
  Map<String, String>? platformEnvironment,
  bool? isWindows,
  String? pathSeparator,
  bool Function(String path)? fileExists,
}) {
  final environment = platformEnvironment ?? Platform.environment;
  final windows = isWindows ?? Platform.isWindows;
  final separator = pathSeparator ?? Platform.pathSeparator;
  bool exists(String path) => fileExists?.call(path) ?? File(path).existsSync();
  final executablePath = resolvedExecutable ?? Platform.resolvedExecutable;
  final executableDir = File(executablePath).parent.path;
  final sidecarName = windows ? 'docflow_sidecar.exe' : 'docflow_sidecar';
  final sidecarRoot = '$executableDir${separator}sidecar';
  final candidates = [
    '$sidecarRoot$separator$sidecarName',
    '$sidecarRoot${separator}docflow_sidecar$separator$sidecarName',
  ];

  for (final candidate in candidates) {
    if (!exists(candidate)) continue;

    final tesseractName = windows ? 'tesseract.exe' : 'tesseract';
    final tesseract =
        '$executableDir${separator}tesseract$separator$tesseractName';
    return SidecarLaunch(
      candidate,
      const [],
      File(candidate).parent.path,
      exists(tesseract) ? {'DOCFLOW_TESSERACT': tesseract} : const {},
    );
  }

  final python = environment['DOCFLOW_PY'];
  final sidecarDir = environment['DOCFLOW_SIDECAR_DIR'];
  if (python != null && sidecarDir != null) {
    return SidecarLaunch(
      python,
      const ['-m', 'docflow_sidecar'],
      sidecarDir,
    );
  }

  return SidecarLaunch(
    devPython,
    const ['-m', 'docflow_sidecar'],
    devSidecarDir,
  );
}
