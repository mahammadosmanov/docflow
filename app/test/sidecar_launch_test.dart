import 'package:docflow/core/ipc/sidecar_launch.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const executable = r'C:\DocFlow\docflow.exe';
  const flatSidecar = r'C:\DocFlow\sidecar\docflow_sidecar.exe';
  const nestedSidecar =
      r'C:\DocFlow\sidecar\docflow_sidecar\docflow_sidecar.exe';
  const tesseract = r'C:\DocFlow\tesseract\tesseract.exe';

  SidecarLaunch resolve(Set<String> files) => resolveSidecarLaunch(
    devPython: r'C:\dev\python.exe',
    devSidecarDir: r'C:\dev\sidecar',
    resolvedExecutable: executable,
    platformEnvironment: const {},
    isWindows: true,
    pathSeparator: r'\',
    fileExists: files.contains,
  );

  test('packaged app uses the required flat sidecar layout first', () {
    final launch = resolve({flatSidecar, nestedSidecar, tesseract});

    expect(launch.executable, flatSidecar);
    expect(launch.workingDir, r'C:\DocFlow\sidecar');
    expect(launch.args, isEmpty);
    expect(launch.environment?['DOCFLOW_TESSERACT'], tesseract);
  });

  test('nested PyInstaller layout remains a defensive package fallback', () {
    final launch = resolve({nestedSidecar, tesseract});

    expect(launch.executable, nestedSidecar);
    expect(launch.workingDir, r'C:\DocFlow\sidecar\docflow_sidecar');
    expect(launch.args, isEmpty);
    expect(launch.environment?['DOCFLOW_TESSERACT'], tesseract);
  });

  test('development environment override still works without a bundle', () {
    final launch = resolveSidecarLaunch(
      devPython: r'C:\fallback\python.exe',
      devSidecarDir: r'C:\fallback\sidecar',
      resolvedExecutable: executable,
      platformEnvironment: const {
        'DOCFLOW_PY': r'C:\override\python.exe',
        'DOCFLOW_SIDECAR_DIR': r'C:\override\sidecar',
      },
      isWindows: true,
      pathSeparator: r'\',
      fileExists: (_) => false,
    );

    expect(launch.executable, r'C:\override\python.exe');
    expect(launch.args, ['-m', 'docflow_sidecar']);
    expect(launch.workingDir, r'C:\override\sidecar');
  });
}
