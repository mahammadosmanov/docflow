/// Port of src-tauri/src/platform/macos/printing.rs: the CUPS print adapter.
///
/// UNVERIFIED: this adapter has not been exercised against a real CUPS
/// scheduler or printer. Command construction, option mapping, and output
/// parsing are unit-tested on any host. Process control uses posix_spawn,
/// waitpid, and kill through dart:ffi (Darwin constants) so that every
/// command is bounded by the same 15 second timeout as the Rust authority.
/// Output is captured to private temporary files, so a chatty command cannot
/// block the poll loop.
library;

import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';
import 'package:path/path.dart' as p;

import '../../print_queue.dart';

const String _lp = '/usr/bin/lp';
const String _lpstat = '/usr/bin/lpstat';
const String _lpoptions = '/usr/bin/lpoptions';
const String _cancel = '/usr/bin/cancel';
const Duration _commandTimeout = Duration(seconds: 15);
const Duration _pollInterval = Duration(milliseconds: 200);
const Duration _spawnPollInterval = Duration(milliseconds: 25);
const List<String> _controlledFormats = ['pdf', 'png', 'jpg', 'jpeg', 'bmp'];

/// Port of `MacOsPrintAdapter`.
class MacOsPrintAdapter implements PrintAdapter {
  const MacOsPrintAdapter();

  @override
  PrinterInventory inventory() {
    final printers = _tryRun(_lpstat, ['-p']);
    final defaults = _tryRun(_lpstat, ['-d']);
    final names = <String>[];
    if (printers != null) {
      for (final line in _lines(printers.stdout)) {
        final name = parsePrinterLine(line);
        if (name != null) {
          names.add(name);
        }
      }
    }
    String? defaultPrinter;
    if (defaults != null) {
      for (final line in _lines(defaults.stdout)) {
        final name = parseDefaultLine(line);
        if (name != null) {
          defaultPrinter = name;
          break;
        }
      }
    }
    return PrinterInventory(
      printers: names,
      defaultPrinter: defaultPrinter,
    ).normalized();
  }

  @override
  PrintJobSupport inspect(String path, String printer, PrintSettings settings) {
    final extension = _extensionOf(path);
    if (!_controlledFormats.contains(extension)) {
      throw PrintFailure.unsupported(
        'macOS CUPS does not accept this document format directly',
      );
    }
    if (!safeCupsDestination(printer)) {
      throw PrintFailure.failed('printer name is invalid');
    }
    _validateReportedOptions(printer, settings);
    if (extension == 'pdf') {
      return PrintJobSupport.controlledPdf();
    }
    return PrintJobSupport.singlePage();
  }

  @override
  void submit(PrintJob job, bool Function() cancelled) {
    if (cancelled()) {
      throw PrintFailure.cancelled(
        'cancelled before CUPS submission',
        uncertain: false,
      );
    }
    final known = inventory();
    if (!known.contains(job.printer)) {
      throw PrintFailure.failed('printer is no longer available');
    }
    inspect(job.path, job.printer, job.settings());
    final args = lpArguments(job);
    final _CommandOutput output;
    try {
      output = _runBounded(_lp, args);
    } on PrintFailure catch (error) {
      if (error.kind == PrintFailureKind.uncertain) {
        rethrow;
      }
      throw PrintFailure.failed('CUPS rejected the print job');
    }
    final jobId = parseJobId(output.stdout, job.printer);
    if (jobId == null) {
      throw PrintFailure.uncertain('CUPS accepted a job without a job ID');
    }
    _waitForJob(job.printer, jobId, job.timeoutMinutes, cancelled);
  }
}

/// Port of `lp_arguments`. The path is delimited by `--` so that a file name
/// beginning with a dash is never interpreted as an option.
List<String> lpArguments(PrintJob job) {
  final args = <String>[
    '-d',
    job.printer,
    '-t',
    'DocFlow - ${_displayName(job.path)}',
  ];
  for (final option in cupsOptions(job.settings())) {
    args.add('-o');
    args.add(option);
  }
  args.add('--');
  args.add(job.path);
  return args;
}

/// Port of `cups_options`.
List<String> cupsOptions(PrintSettings settings) {
  final options = <String>['job-sheets=none'];
  if (settings.paper != 'auto') {
    options.add('media=${settings.paper}');
  }
  switch (settings.orientation) {
    case 'portrait':
      options.add('orientation-requested=3');
    case 'landscape':
      options.add('orientation-requested=4');
  }
  switch (settings.scaling) {
    case 'fit':
      options.add('fit-to-page');
    case 'actual':
      options.add('print-scaling=none');
    case 'shrink':
      options.add('print-scaling=auto-fit');
  }
  switch (settings.color) {
    case 'color':
      options.add('print-color-mode=color');
    case 'grayscale':
      options.add('print-color-mode=monochrome');
  }
  switch (settings.duplex) {
    case 'simplex':
      options.add('sides=one-sided');
    case 'long':
      options.add('sides=two-sided-long-edge');
    case 'short':
      options.add('sides=two-sided-short-edge');
  }
  if (settings.pageRange != 'all') {
    options.add('page-ranges=${settings.pageRange}');
  }
  return options;
}

/// Port of `validate_reported_options`. If CUPS reports no legacy PPD options
/// (driverless destinations), the scheduler validates standard IPP options on
/// submit instead.
void _validateReportedOptions(String printer, PrintSettings settings) {
  final _CommandOutput output;
  try {
    output = _runBounded(_lpoptions, ['-p', printer, '-l']);
  } on PrintFailure {
    return;
  }
  final reported = _asciiLower(output.stdout);
  if (reported.trim().isEmpty) {
    return;
  }
  if (settings.paper != 'auto' &&
      !reported.contains(_asciiLower(settings.paper))) {
    throw PrintFailure.unsupportedOptions(
      'the selected paper is not reported by this printer',
    );
  }
  if (settings.duplex != 'printer' &&
      !reported.contains('duplex') &&
      !reported.contains('sides')) {
    throw PrintFailure.unsupportedOptions(
      'the selected duplex mode is not reported by this printer',
    );
  }
  if (settings.color != 'printer' &&
      !reported.contains('color') &&
      !reported.contains('monochrome')) {
    throw PrintFailure.unsupportedOptions(
      'the selected color mode is not reported by this printer',
    );
  }
}

/// Port of `wait_for_job`: polls `lpstat -W not-completed` until the named
/// job leaves the queue. Cancellation and deadline attempt `cancel` once.
void _waitForJob(
  String printer,
  int jobId,
  int timeoutMinutes,
  bool Function() cancelled,
) {
  final jobName = '$printer-$jobId';
  final deadline = DateTime.now().add(Duration(minutes: timeoutMinutes));
  while (true) {
    if (cancelled()) {
      try {
        _runBounded(_cancel, [jobName]);
      } on PrintFailure {
        throw PrintFailure.uncertain(
          'CUPS could not confirm cancellation of the active job',
        );
      }
      throw activeCupsCancellation();
    }
    if (!DateTime.now().isBefore(deadline)) {
      try {
        _runBounded(_cancel, [jobName]);
      } on PrintFailure {
        // The timeout outcome is reported regardless of the cancel result.
      }
      throw PrintFailure.uncertain(
        'CUPS did not finish the print job before its timeout',
      );
    }
    final _CommandOutput output;
    try {
      output = _runBounded(_lpstat, ['-W', 'not-completed', '-o', printer]);
    } on PrintFailure {
      throw PrintFailure.uncertain('CUPS job state could not be read');
    }
    final active = _lines(output.stdout).any(
      (line) => _firstToken(line) == jobName,
    );
    if (!active) {
      return;
    }
    sleep(_pollInterval);
  }
}

PrintFailure activeCupsCancellation() => PrintFailure.cancelled(
  'CUPS accepted cancellation of an active print job',
  uncertain: true,
);

// ---- Bounded command execution (posix_spawn over dart:ffi) -----------------

class _CommandOutput {
  const _CommandOutput(this.stdout);

  final String stdout;
}

/// Runs [program] with [args] and a 15 second bound. Mirrors `run_bounded`:
/// spawn failure is `failed`, a non-zero exit is `failed`, and a command that
/// exceeds the bound is killed and reported as `uncertain`.
_CommandOutput _runBounded(String program, List<String> args) {
  final scratch = Directory.systemTemp.createTempSync('docflow-cups-');
  final stdoutFile = File(p.join(scratch.path, 'stdout'));
  final stderrFile = File(p.join(scratch.path, 'stderr'));
  try {
    final status = _spawnAndWait(
      program,
      args,
      stdoutPath: stdoutFile.path,
      stderrPath: stderrFile.path,
    );
    if (status != 0) {
      throw PrintFailure.failed('native CUPS command failed');
    }
    final stdout = stdoutFile.existsSync()
        ? utf8.decode(stdoutFile.readAsBytesSync(), allowMalformed: true)
        : '';
    return _CommandOutput(stdout);
  } finally {
    scratch.deleteSync(recursive: true);
  }
}

/// Returns `null` on any failure, mirroring `run_bounded(...).ok()`.
_CommandOutput? _tryRun(String program, List<String> args) {
  try {
    return _runBounded(program, args);
  } on PrintFailure {
    return null;
  }
}

/// Spawns the command with `LPDEST` and `PRINTER` removed from the
/// environment, waits up to [_commandTimeout], and returns the exit status.
int _spawnAndWait(
  String program,
  List<String> args, {
  required String stdoutPath,
  required String stderrPath,
}) {
  final libc = DynamicLibrary.process();
  final posixSpawn = libc.lookupFunction<_PosixSpawnN, _PosixSpawnD>(
    'posix_spawn',
  );
  final fileActionsInit = libc.lookupFunction<_FaInitN, _FaInitD>(
    'posix_spawn_file_actions_init',
  );
  final fileActionsAddOpen = libc.lookupFunction<_FaAddOpenN, _FaAddOpenD>(
    'posix_spawn_file_actions_addopen',
  );
  final fileActionsDestroy = libc.lookupFunction<_FaDestroyN, _FaDestroyD>(
    'posix_spawn_file_actions_destroy',
  );
  final waitpid = libc.lookupFunction<_WaitpidN, _WaitpidD>('waitpid');
  final kill = libc.lookupFunction<_KillN, _KillD>('kill');

  final argv = [program, ...args];
  final env = Map<String, String>.from(Platform.environment)
    ..remove('LPDEST')
    ..remove('PRINTER');
  final envList = [for (final entry in env.entries) '${entry.key}=${entry.value}'];

  // posix_spawn_file_actions_t is a pointer on Darwin but a large struct on
  // glibc, so reserve a generous zeroed block that fits either layout.
  final fileActions = calloc<Pointer<Void>>(32);
  final pid = calloc<Int32>();
  final status = calloc<Int32>();
  final cArgv = _toNativeStringArray(argv);
  final cEnv = _toNativeStringArray(envList);
  final cPath = program.toNativeUtf8();
  final cStdin = '/dev/null'.toNativeUtf8();
  final cStdout = stdoutPath.toNativeUtf8();
  final cStderr = stderrPath.toNativeUtf8();
  try {
    if (fileActionsInit(fileActions) != 0) {
      throw PrintFailure.failed('native CUPS command is unavailable');
    }
    try {
      if (fileActionsAddOpen(fileActions, 0, cStdin, _oRdOnly, 0) != 0 ||
          fileActionsAddOpen(
                fileActions,
                1,
                cStdout,
                _oWrOnlyCreateTrunc,
                _mode0600,
              ) !=
              0 ||
          fileActionsAddOpen(
                fileActions,
                2,
                cStderr,
                _oWrOnlyCreateTrunc,
                _mode0600,
              ) !=
              0) {
        throw PrintFailure.failed('native CUPS command is unavailable');
      }
      final spawnStatus = posixSpawn(
        pid,
        cPath,
        fileActions.cast(),
        nullptr,
        cArgv,
        cEnv,
      );
      if (spawnStatus != 0) {
        throw PrintFailure.failed('native CUPS command is unavailable');
      }
    } finally {
      fileActionsDestroy(fileActions);
    }

    final child = pid.value;
    final deadline = DateTime.now().add(_commandTimeout);
    while (true) {
      final reaped = waitpid(child, status, _wnohang);
      if (reaped == child) {
        final raw = status.value;
        final exited = (raw & 0x7f) == 0;
        return exited ? (raw >> 8) & 0xff : -1;
      }
      if (reaped == -1) {
        kill(child, _sigKill);
        waitpid(child, status, 0);
        throw PrintFailure.failed('native CUPS command failed');
      }
      if (DateTime.now().isBefore(deadline)) {
        sleep(_spawnPollInterval);
        continue;
      }
      kill(child, _sigKill);
      waitpid(child, status, 0);
      throw PrintFailure.uncertain('native CUPS command timed out');
    }
  } finally {
    calloc.free(cPath);
    calloc.free(cStdin);
    calloc.free(cStdout);
    calloc.free(cStderr);
    _freeNativeStringArray(cArgv, argv.length);
    _freeNativeStringArray(cEnv, envList.length);
    calloc.free(fileActions);
    calloc.free(pid);
    calloc.free(status);
  }
}

// Open-flag values differ between Darwin and Linux. The adapter targets macOS,
// but the same code path must not write through the wrong flags elsewhere.
final bool _isDarwin = Platform.isMacOS;
final int _oRdOnly = 0x0000;
final int _oWrOnlyCreateTrunc = _isDarwin
    ? (0x0001 | 0x0200 | 0x0400) // Darwin O_WRONLY | O_CREAT | O_TRUNC
    : (0x0001 | 0x0040 | 0x0200); // Linux O_WRONLY | O_CREAT | O_TRUNC
const int _mode0600 = 0x180;
const int _wnohang = 0x0001;
const int _sigKill = 9;

typedef _PosixSpawnN =
    Int32 Function(
      Pointer<Int32> pid,
      Pointer<Utf8> path,
      Pointer<Void> fileActions,
      Pointer<Void> attrp,
      Pointer<Pointer<Utf8>> argv,
      Pointer<Pointer<Utf8>> envp,
    );
typedef _PosixSpawnD =
    int Function(
      Pointer<Int32> pid,
      Pointer<Utf8> path,
      Pointer<Void> fileActions,
      Pointer<Void> attrp,
      Pointer<Pointer<Utf8>> argv,
      Pointer<Pointer<Utf8>> envp,
    );
typedef _FaInitN = Int32 Function(Pointer<Pointer<Void>> fileActions);
typedef _FaInitD = int Function(Pointer<Pointer<Void>> fileActions);
typedef _FaAddOpenN =
    Int32 Function(
      Pointer<Pointer<Void>> fileActions,
      Int32 fd,
      Pointer<Utf8> path,
      Int32 oflag,
      Uint16 mode,
    );
typedef _FaAddOpenD =
    int Function(
      Pointer<Pointer<Void>> fileActions,
      int fd,
      Pointer<Utf8> path,
      int oflag,
      int mode,
    );
typedef _FaDestroyN = Int32 Function(Pointer<Pointer<Void>> fileActions);
typedef _FaDestroyD = int Function(Pointer<Pointer<Void>> fileActions);
typedef _WaitpidN =
    Int32 Function(Int32 pid, Pointer<Int32> status, Int32 options);
typedef _WaitpidD = int Function(int pid, Pointer<Int32> status, int options);
typedef _KillN = Int32 Function(Int32 pid, Int32 signal);
typedef _KillD = int Function(int pid, int signal);

Pointer<Pointer<Utf8>> _toNativeStringArray(List<String> values) {
  final array = calloc<Pointer<Utf8>>(values.length + 1);
  for (var i = 0; i < values.length; i++) {
    array[i] = values[i].toNativeUtf8();
  }
  array[values.length] = nullptr;
  return array;
}

void _freeNativeStringArray(Pointer<Pointer<Utf8>> array, int length) {
  for (var i = 0; i < length; i++) {
    calloc.free(array[i]);
  }
  calloc.free(array);
}

// ---- Parsers and validation ------------------------------------------------

String? parsePrinterLine(String line) {
  if (!line.startsWith('printer ')) {
    return null;
  }
  final name = _firstToken(line.substring('printer '.length));
  if (name == null || !safeCupsDestination(name)) {
    return null;
  }
  return name;
}

String? parseDefaultLine(String line) {
  const prefix = 'system default destination: ';
  if (!line.startsWith(prefix)) {
    return null;
  }
  final name = line.substring(prefix.length);
  return safeCupsDestination(name) ? name : null;
}

int? parseJobId(String output, String printer) {
  final prefix = 'request id is $printer-';
  for (final line in _lines(output)) {
    if (!line.startsWith(prefix)) {
      continue;
    }
    final token = _firstToken(line.substring(prefix.length));
    if (token == null || !RegExp(r'^\+?[0-9]+$').hasMatch(token)) {
      return null;
    }
    final value = int.parse(token.replaceFirst('+', ''));
    return value <= 0xFFFFFFFF ? value : null;
  }
  return null;
}

/// Port of `safe_cups_destination`: 1 to 127 bytes, no control characters,
/// no space, tab, slash, or hash.
bool safeCupsDestination(String name) {
  if (name.isEmpty || utf8.encode(name).length > 127) {
    return false;
  }
  for (final rune in name.runes) {
    if (_isControl(rune) || rune == 0x20 || rune == 0x09 || rune == 0x2F || rune == 0x23) {
      return false;
    }
  }
  return true;
}

bool _isControl(int rune) => rune < 0x20 || (rune >= 0x7F && rune <= 0x9F);

String _displayName(String path) {
  final name = p.basename(path);
  return name.isEmpty || name == '/' ? 'document' : name;
}

String _extensionOf(String path) {
  final extension = p.extension(path);
  return _asciiLower(extension.isEmpty ? '' : extension.substring(1));
}

String _asciiLower(String value) => String.fromCharCodes(
  value.codeUnits.map((unit) => unit >= 0x41 && unit <= 0x5A ? unit + 32 : unit),
);

List<String> _lines(String text) => [
  for (final line in text.split('\n'))
    line.endsWith('\r') ? line.substring(0, line.length - 1) : line,
];

String? _firstToken(String text) {
  final match = RegExp(r'\S+').firstMatch(text);
  return match?.group(0);
}
