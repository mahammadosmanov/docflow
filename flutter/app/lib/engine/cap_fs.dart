/// Port of src-tauri/src/cap_fs.rs: capability-scoped filesystem access.
///
/// Every mutation and every read in the engine's safety-critical paths goes
/// through a [CapabilityRoot] that was opened once (canonical path, no
/// follow). Each later component is resolved with `openat(2)` relative to the
/// pinned directory descriptor using `O_NOFOLLOW`, so a parent swapped for a
/// symlink after authorization cannot redirect the effect.
///
/// Platform status:
///  * Linux x86_64: fully implemented through `dart:ffi` against glibc and
///    exercised by the E2 tests.
///  * macOS (BSD `*at` family, `renameatx_np`, `fstat$INODE64`, xattr with
///    position/options): implemented but UNVERIFIED on macOS hardware.
///  * Linux arm64: stat layout implemented but UNVERIFIED.
///  * Windows (NtCreateFile/NtSetInformationFile relative handles, ADS,
///    FILE_BASIC_INFO attributes): UNVERIFIED. Behind [CapabilityPlatform]
///    as [_WindowsCapabilityPlatform], which refuses to run on this host.
library;

import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';

import 'package:crypto/crypto.dart' as crypto;
import 'package:ffi/ffi.dart';

import 'ooxml.dart' show maxPackageBytes;
import 'journal.dart'
    show
        AnchoredPathPort,
        ArtifactParentAlreadyExists,
        ArtifactParentCreated,
        ArtifactParentCreatedIdentityUnknown,
        ArtifactParentCreateOutcome,
        AuthorizedPathPort,
        CapabilityRootPort,
        FilesystemObjectIdentity,
        FsErrorKind,
        FsIoException,
        UnixObjectIdentity;

/// Hard limits from cap_fs.rs.
const int maxSecurityAuxiliaryRecords = 128;
const int maxSecurityAuxiliaryBytes = 1024 * 1024;

const List<String> s5RetainedPrefixes = [
  '.docflow-s5-backup-',
  '.docflow-s5-private-output-',
  '.docflow-s5-retained-original-',
  '.docflow-s5-retained-applied-',
];

// ---------------------------------------------------------------------------
// io::Error equivalent
// ---------------------------------------------------------------------------

/// Dart counterpart of `std::io::Error` used by the capability layer. It
/// extends the journal's [FsIoException] so both layers share one error type.
/// [osError] mirrors `raw_os_error()`, which the EXDEV and Windows-code
/// checks rely on.
class IoError extends FsIoException {
  const IoError(super.kind, super.message, {this.osError});

  factory IoError.fromErrno(int errno) => IoError(
    _kindForErrno(errno),
    'os error $errno',
    osError: errno,
  );

  final int? osError;

  @override
  String toString() => osError == null ? message : '$message (os error $osError)';
}

FsErrorKind _kindForErrno(int errno) {
  final sys = _Sys.current;
  if (errno == sys.eNoent) return FsErrorKind.notFound;
  if (errno == sys.eAcces || errno == sys.ePerm) {
    return FsErrorKind.permissionDenied;
  }
  if (errno == sys.eExist) return FsErrorKind.alreadyExists;
  if (errno == sys.eInval) return FsErrorKind.invalidInput;
  if (errno == sys.eNotSup) return FsErrorKind.unsupported;
  if (errno == sys.eNotDir) return FsErrorKind.other;
  if (errno == sys.eNotEmpty) return FsErrorKind.directoryNotEmpty;
  return FsErrorKind.other;
}

// ---------------------------------------------------------------------------
// Rust-compatible Path helpers. Paths are kept as Dart Strings. Components
// follow std::path rules: duplicate and trailing separators disappear, `.` is
// dropped except as a leading CurDir, `..` is kept as ParentDir.
// ---------------------------------------------------------------------------

enum _CompKind { rootDir, curDir, parentDir, normal }

class _Comp {
  const _Comp(this.kind, this.name);
  final _CompKind kind;
  final String name;
}

List<_Comp> _components(String path) {
  final out = <_Comp>[];
  if (path.startsWith('/')) {
    out.add(const _Comp(_CompKind.rootDir, '/'));
  }
  final segments = path.split('/');
  for (var i = 0; i < segments.length; i++) {
    final segment = segments[i];
    if (segment.isEmpty) continue;
    if (segment == '.') {
      if (i == 0) out.add(const _Comp(_CompKind.curDir, '.'));
      continue;
    }
    if (segment == '..') {
      out.add(const _Comp(_CompKind.parentDir, '..'));
      continue;
    }
    out.add(_Comp(_CompKind.normal, segment));
  }
  return out;
}

String _renderComponents(bool absolute, List<_Comp> comps) {
  final names = comps
      .where((c) => c.kind != _CompKind.rootDir)
      .map((c) => c.name)
      .toList();
  if (absolute) return '/${names.join('/')}';
  return names.join('/');
}

/// `Path::is_absolute` on the POSIX platform.
bool pathIsAbsolute(String path) => path.startsWith('/');

/// `Path::parent`. Returns null for a root or empty path.
String? pathParent(String path) {
  final comps = _components(path);
  if (comps.isEmpty) return null;
  final last = comps.removeLast();
  if (last.kind == _CompKind.rootDir) return null;
  return _renderComponents(pathIsAbsolute(path), comps);
}

/// `Path::file_name`: only a trailing Normal component yields a name.
String? pathFileName(String path) {
  final comps = _components(path);
  if (comps.isEmpty) return null;
  final last = comps.last;
  return last.kind == _CompKind.normal ? last.name : null;
}

/// `Path::extension` with the std rules for leading dots and `..`.
String? pathExtension(String path) {
  final name = pathFileName(path);
  if (name == null) return null;
  if (name == '..') return null;
  final dot = name.lastIndexOf('.');
  if (dot <= 0) return null;
  return name.substring(dot + 1);
}

/// `Path::starts_with`, component-wise.
bool pathStartsWith(String path, String base) {
  final comps = _components(path);
  final baseComps = _components(base);
  if (baseComps.length > comps.length) return false;
  for (var i = 0; i < baseComps.length; i++) {
    if (baseComps[i].kind != comps[i].kind ||
        baseComps[i].name != comps[i].name) {
      return false;
    }
  }
  return true;
}

/// `Path::strip_prefix`, component-wise. Returns the relative remainder.
String? pathStripPrefix(String path, String base) {
  if (!pathStartsWith(path, base)) return null;
  final comps = _components(path).skip(_components(base).length).toList();
  return comps.map((c) => c.name).join('/');
}

/// `Path::join`.
String pathJoin(String base, String child) {
  if (pathIsAbsolute(child)) return child;
  if (base.isEmpty) return child;
  if (base.endsWith('/')) return '$base$child';
  return '$base/$child';
}

/// `Path::components` as a list of normal names (used by cap_fs validators).
List<String> pathNormalNames(String path) =>
    _components(path).where((c) => c.kind == _CompKind.normal).map((c) => c.name).toList();

// ---------------------------------------------------------------------------
// Platform interface and the Linux/macOS (POSIX) and Windows implementations.
// ---------------------------------------------------------------------------

/// Directory-access level of a capability root.
enum DirectoryAccess { readOnly, readWrite }

/// Result of `fstat` on an owned handle (subset of `std::fs::Metadata`).
class HandleStat {
  const HandleStat({
    required this.dev,
    required this.ino,
    required this.mode,
    required this.nlink,
    required this.uid,
    required this.gid,
    required this.size,
    required this.mtimeNs,
  });

  final int dev;
  final int ino;
  final int mode;
  final int nlink;
  final int uid;
  final int gid;
  final int size;

  /// `Metadata::modified()` in nanoseconds since the epoch.
  final int mtimeNs;

  bool get isFile => (mode & 0xF000) == 0x8000;
  bool get isDir => (mode & 0xF000) == 0x4000;

  /// `Permissions::mode() & 0o7777`.
  int get permissionBits => mode & 0x0FFF;
}

/// Platform primitives behind the capability layer. Everything above this
/// interface is platform-neutral Dart; everything below it is OS-specific.
abstract interface class CapabilityPlatform {
  OsHandle openAmbientDirectory(String path);
  OsHandle openDirectoryAt(OsHandle parent, String name);
  OsHandle openFileAt(OsHandle parent, String name, {required bool mutable});
  OsHandle createFileAt(OsHandle parent, String name);
  void createDirectoryAt(OsHandle parent, String name);
  void unlinkAt(OsHandle parent, String name, {required bool directory});
  void renameAt(
    OsHandle sourceParent,
    String source,
    OsHandle destinationParent,
    String destination, {
    required bool replace,
  });
  List<String> entryNames(OsHandle directory);
  HandleStat statHandle(OsHandle handle);
  FilesystemObjectIdentity identityOf(OsHandle handle);
  int readInto(OsHandle handle, Uint8List buffer);
  void writeAll(OsHandle handle, Uint8List bytes);
  void seekTo(OsHandle handle, int position);
  void syncHandle(OsHandle handle);
  void truncateHandle(OsHandle handle, int length);
  void chmodHandle(OsHandle handle, int mode);
  void chownHandle(OsHandle handle, int uid, int gid);
  OsHandle duplicateHandle(OsHandle handle);
  void closeHandle(OsHandle handle);
  List<(List<int>, List<int>)> listExtendedAttributes(OsHandle handle);
  void setExtendedAttribute(OsHandle handle, List<int> name, List<int> value);
  int effectiveUid();
  int effectiveGid();
  OsHandle openPathReadOnly(String path, {required bool noFollow});
  OsHandle createPathExclusive(String path);
  void linkPath(String source, String destination);
  void renamePathNoReplace(String source, String destination);
}

CapabilityPlatform _platformInstance() {
  if (Platform.isWindows) return _WindowsCapabilityPlatform();
  if (Platform.isLinux) {
    final abi = Abi.current();
    if (abi != Abi.linuxX64 && abi != Abi.linuxArm64) {
      throw UnsupportedError('capability layer supports Linux x64/arm64 only');
    }
  }
  return _PosixCapabilityPlatform();
}

CapabilityPlatform get _platform => _platformCache ??= _platformInstance();
CapabilityPlatform? _platformCache;

// ---------------------------------------------------------------------------
// libc bindings (POSIX).
// ---------------------------------------------------------------------------

// open/openat are variadic: the mode must travel as a variadic argument or
// arm64 Darwin reads garbage from the stack.
typedef _OpenatNative =
    Int32 Function(Int32, Pointer<Utf8>, Int32, VarArgs<(Int32,)>);
typedef _OpenatDart = int Function(int, Pointer<Utf8>, int, int);
typedef _CloseNative = Int32 Function(Int32);
typedef _CloseDart = int Function(int);
typedef _IoNative = IntPtr Function(Int32, Pointer<Uint8>, IntPtr);
typedef _IoDart = int Function(int, Pointer<Uint8>, int);
typedef _LseekNative = Int64 Function(Int32, Int64, Int32);
typedef _LseekDart = int Function(int, int, int);
typedef _FsyncNative = Int32 Function(Int32);
typedef _FsyncDart = int Function(int);
typedef _FtruncateNative = Int32 Function(Int32, Int64);
typedef _FtruncateDart = int Function(int, int);
typedef _FchmodNative = Int32 Function(Int32, Uint32);
typedef _FchmodDart = int Function(int, int);
typedef _FchownNative = Int32 Function(Int32, Uint32, Uint32);
typedef _FchownDart = int Function(int, int, int);
typedef _FstatNative = Int32 Function(Int32, Pointer<Uint8>);
typedef _FstatDart = int Function(int, Pointer<Uint8>);
typedef _MkdiratNative = Int32 Function(Int32, Pointer<Utf8>, Uint32);
typedef _MkdiratDart = int Function(int, Pointer<Utf8>, int);
typedef _UnlinkatNative = Int32 Function(Int32, Pointer<Utf8>, Int32);
typedef _UnlinkatDart = int Function(int, Pointer<Utf8>, int);
typedef _RenameatNative = Int32 Function(Int32, Pointer<Utf8>, Int32, Pointer<Utf8>);
typedef _RenameatDart = int Function(int, Pointer<Utf8>, int, Pointer<Utf8>);
typedef _Renameat2Native = Int32 Function(
  Int32,
  Pointer<Utf8>,
  Int32,
  Pointer<Utf8>,
  Uint32,
);
typedef _Renameat2Dart = int Function(int, Pointer<Utf8>, int, Pointer<Utf8>, int);
typedef _LinkNative = Int32 Function(Pointer<Utf8>, Pointer<Utf8>);
typedef _LinkDart = int Function(Pointer<Utf8>, Pointer<Utf8>);
typedef _FlistxattrNative = IntPtr Function(Int32, Pointer<Uint8>, IntPtr);
typedef _FlistxattrDart = int Function(int, Pointer<Uint8>, int);
typedef _FlistxattrDarwinNative = IntPtr Function(Int32, Pointer<Uint8>, IntPtr, Int32);
typedef _FlistxattrDarwinDart = int Function(int, Pointer<Uint8>, int, int);
typedef _FgetxattrNative = IntPtr Function(Int32, Pointer<Uint8>, Pointer<Uint8>, IntPtr);
typedef _FgetxattrDart = int Function(int, Pointer<Uint8>, Pointer<Uint8>, int);
typedef _FgetxattrDarwinNative = IntPtr Function(
  Int32,
  Pointer<Uint8>,
  Pointer<Uint8>,
  IntPtr,
  Uint32,
  Int32,
);
typedef _FgetxattrDarwinDart = int Function(
  int,
  Pointer<Uint8>,
  Pointer<Uint8>,
  int,
  int,
  int,
);
typedef _FsetxattrDarwinDart = int Function(
  int,
  Pointer<Uint8>,
  Pointer<Uint8>,
  int,
  int,
  int,
);
typedef _FsetxattrNative = Int32 Function(Int32, Pointer<Uint8>, Pointer<Uint8>, IntPtr, Int32);
typedef _FsetxattrDart = int Function(int, Pointer<Uint8>, Pointer<Uint8>, int, int);
typedef _FsetxattrDarwinNative = Int32 Function(
  Int32,
  Pointer<Uint8>,
  Pointer<Uint8>,
  IntPtr,
  Uint32,
  Int32,
);
typedef _ErrnoNative = Pointer<Int32> Function();
typedef _IdNative = Uint32 Function();
typedef _IdDart = int Function();
typedef _DupNative = Int32 Function(Int32);
typedef _DupDart = int Function(int);
typedef _FdopendirNative = Pointer<Void> Function(Int32);
typedef _FdopendirDart = Pointer<Void> Function(int);
typedef _ReaddirNative = Pointer<Uint8> Function(Pointer<Void>);
typedef _ReaddirDart = Pointer<Uint8> Function(Pointer<Void>);
typedef _ClosedirNative = Int32 Function(Pointer<Void>);
typedef _ClosedirDart = int Function(Pointer<Void>);
typedef _OpenNative = Int32 Function(Pointer<Utf8>, Int32, VarArgs<(Int32,)>);
typedef _OpenDart = int Function(Pointer<Utf8>, int, int);

/// Per-OS constants and resolved symbols. Only Linux is verified on this host.
class _Sys {
  _Sys._(this._lib, this.isDarwin, this.isLinuxArm64)
    : oRdonly = 0,
      oRdwr = 2,
      oCreat = isDarwin ? 0x200 : 0x40,
      oExcl = isDarwin ? 0x800 : 0x80,
      oDirectory = isDarwin ? 0x100000 : 0x10000,
      oNofollow = isDarwin ? 0x100 : 0x20000,
      oCloexec = isDarwin ? 0x1000000 : 0x80000,
      atFdcwd = isDarwin ? -2 : -100,
      atRemovedir = isDarwin ? 0x80 : 0x200,
      dNameOffset = isDarwin ? 21 : 19,
      eNoent = 2,
      ePerm = 1,
      eAcces = 13,
      eIntr = 4,
      eExist = 17,
      eXdev = 18,
      eNotDir = 20,
      eInval = 22,
      eNotEmpty = isDarwin ? 66 : 39,
      eNotSup = isDarwin ? 45 : 95,
      renameNoReplace = isDarwin ? 0x4 : 1,
      seekSet = 0,
      sIfMt = 0xF000,
      sIfReg = 0x8000 {
    openat = _lib.lookupFunction<_OpenatNative, _OpenatDart>('openat');
    close = _lib.lookupFunction<_CloseNative, _CloseDart>('close');
    read = _lib.lookupFunction<_IoNative, _IoDart>('read');
    write = _lib.lookupFunction<_IoNative, _IoDart>('write');
    lseek = _lib.lookupFunction<_LseekNative, _LseekDart>('lseek');
    fsync = _lib.lookupFunction<_FsyncNative, _FsyncDart>('fsync');
    ftruncate = _lib.lookupFunction<_FtruncateNative, _FtruncateDart>('ftruncate');
    fchmod = _lib.lookupFunction<_FchmodNative, _FchmodDart>('fchmod');
    fchown = _lib.lookupFunction<_FchownNative, _FchownDart>('fchown');
    fstat = _lib.lookupFunction<_FstatNative, _FstatDart>(
      isDarwin && _lib.providesSymbol('fstat\$INODE64') ? 'fstat\$INODE64' : 'fstat',
    );
    mkdirat = _lib.lookupFunction<_MkdiratNative, _MkdiratDart>('mkdirat');
    unlinkat = _lib.lookupFunction<_UnlinkatNative, _UnlinkatDart>('unlinkat');
    renameat = _lib.lookupFunction<_RenameatNative, _RenameatDart>('renameat');
    renameNoReplaceAt = _lib.lookupFunction<_Renameat2Native, _Renameat2Dart>(
      isDarwin ? 'renameatx_np' : 'renameat2',
    );
    link = _lib.lookupFunction<_LinkNative, _LinkDart>('link');
    dup = _lib.lookupFunction<_DupNative, _DupDart>('dup');
    fdopendir = _lib.lookupFunction<_FdopendirNative, _FdopendirDart>('fdopendir');
    readdir = _lib.lookupFunction<_ReaddirNative, _ReaddirDart>('readdir');
    closedir = _lib.lookupFunction<_ClosedirNative, _ClosedirDart>('closedir');
    geteuid = _lib.lookupFunction<_IdNative, _IdDart>('geteuid');
    getegid = _lib.lookupFunction<_IdNative, _IdDart>('getegid');
    errnoLocation = _lib.lookupFunction<_ErrnoNative, _ErrnoNative>(
      isDarwin ? '__error' : '__errno_location',
    );
    open = _lib.lookupFunction<_OpenNative, _OpenDart>('open');
    flistxattrLinux = isDarwin
        ? null
        : _lib.lookupFunction<_FlistxattrNative, _FlistxattrDart>('flistxattr');
    flistxattrDarwin = isDarwin
        ? _lib.lookupFunction<_FlistxattrDarwinNative, _FlistxattrDarwinDart>(
            'flistxattr',
          )
        : null;
    fgetxattrLinux = isDarwin
        ? null
        : _lib.lookupFunction<_FgetxattrNative, _FgetxattrDart>('fgetxattr');
    fgetxattrDarwin = isDarwin
        ? _lib.lookupFunction<_FgetxattrDarwinNative, _FgetxattrDarwinDart>(
            'fgetxattr',
          )
        : null;
    fsetxattrLinux = isDarwin
        ? null
        : _lib.lookupFunction<_FsetxattrNative, _FsetxattrDart>('fsetxattr');
    fsetxattrDarwin = isDarwin
        ? _lib.lookupFunction<_FsetxattrDarwinNative, _FsetxattrDarwinDart>(
            'fsetxattr',
          )
        : null;
  }

  static final _Sys current = _Sys._(
    DynamicLibrary.process(),
    Platform.isMacOS,
    Platform.isLinux && Abi.current() == Abi.linuxArm64,
  );

  final DynamicLibrary _lib;
  final bool isDarwin;
  final bool isLinuxArm64;

  final int oRdonly;
  final int oRdwr;
  final int oCreat;
  final int oExcl;
  final int oDirectory;
  final int oNofollow;
  final int oCloexec;
  final int atFdcwd;
  final int atRemovedir;
  final int dNameOffset;
  final int eNoent;
  final int ePerm;
  final int eAcces;
  final int eIntr;
  final int eExist;
  final int eXdev;
  final int eNotDir;
  final int eInval;
  final int eNotEmpty;
  final int eNotSup;
  final int renameNoReplace;
  final int seekSet;
  final int sIfMt;
  final int sIfReg;

  late final _OpenatDart openat;
  late final _CloseDart close;
  late final _IoDart read;
  late final _IoDart write;
  late final _LseekDart lseek;
  late final _FsyncDart fsync;
  late final _FtruncateDart ftruncate;
  late final _FchmodDart fchmod;
  late final _FchownDart fchown;
  late final _FstatDart fstat;
  late final _MkdiratDart mkdirat;
  late final _UnlinkatDart unlinkat;
  late final _RenameatDart renameat;
  late final _Renameat2Dart renameNoReplaceAt;
  late final _LinkDart link;
  late final _DupDart dup;
  late final _FdopendirDart fdopendir;
  late final _ReaddirDart readdir;
  late final _ClosedirDart closedir;
  late final _IdDart geteuid;
  late final _IdDart getegid;
  late final Pointer<Int32> Function() errnoLocation;
  late final _OpenDart open;
  late final _FlistxattrDart? flistxattrLinux;
  late final _FlistxattrDarwinDart? flistxattrDarwin;
  late final _FgetxattrDart? fgetxattrLinux;
  late final _FgetxattrDarwinDart? fgetxattrDarwin;
  late final _FsetxattrDart? fsetxattrLinux;
  late final _FsetxattrDarwinDart? fsetxattrDarwin;

  int get errno => errnoLocation().value;
  set errno(int value) => errnoLocation().value = value;
}

/// Thin owner of a file descriptor. Closed explicitly; a [Finalizer] is a
/// safety net for leaked descriptors, never the primary close path.
final class OsHandle {
  OsHandle._(this._fd) {
    _closer.attach(this, _fd, detach: this);
  }

  static final Finalizer<int> _closer = Finalizer<int>((fd) {
    _Sys.current.close(fd);
  });

  final int _fd;
  bool _closed = false;

  /// Wraps an already-open descriptor. Used only by the platform layer.
  factory OsHandle.adopt(int fd) => OsHandle._(fd);

  int get fd {
    if (_closed) {
      throw const IoError(FsErrorKind.other, 'file handle is closed');
    }
    return _fd;
  }

  bool get isClosed => _closed;

  void close() {
    if (_closed) return;
    _closed = true;
    _closer.detach(this);
    _Sys.current.close(_fd);
  }

  // Operations are delegated to the platform so the Rust call sites read the
  // same way: `handle.read`, `handle.writeAll`, `handle.syncAll`, ...
  int read(Uint8List buffer) => _platform.readInto(this, buffer);
  void writeAll(Uint8List bytes) => _platform.writeAll(this, bytes);
  void seek(int position) => _platform.seekTo(this, position);
  void syncAll() => _platform.syncHandle(this);
  void setLen(int length) => _platform.truncateHandle(this, length);
  HandleStat metadata() => _platform.statHandle(this);
  void setPermissionBits(int mode) => _platform.chmodHandle(this, mode);
  OsHandle tryClone() => _platform.duplicateHandle(this);
}

// ---------------------------------------------------------------------------
// POSIX implementation (Linux verified, macOS UNVERIFIED).
// ---------------------------------------------------------------------------

final class _PosixCapabilityPlatform implements CapabilityPlatform {
  _Sys get _s => _Sys.current;

  IoError _lastOsError() => IoError.fromErrno(_s.errno);

  static Pointer<Utf8> _cString(String value) {
    if (value.isEmpty || value.codeUnits.contains(0)) {
      throw const IoError(FsErrorKind.invalidInput, 'name contains NUL');
    }
    return value.toNativeUtf8();
  }

  /// Mirrors `unix_name`: empty names, NUL bytes and "." are rejected.
  static String _checkedName(String name) {
    if (name.isEmpty || name.codeUnits.contains(0) || name == '.') {
      throw const IoError(
        FsErrorKind.invalidInput,
        'invalid capability-relative name',
      );
    }
    return name;
  }

  OsHandle _adoptOrThrow(int fd) {
    if (fd < 0) throw _lastOsError();
    return OsHandle.adopt(fd);
  }

  @override
  OsHandle openAmbientDirectory(String path) {
    final c = _cString(path);
    try {
      return _adoptOrThrow(
        _s.openat(
          _s.atFdcwd,
          c,
          _s.oRdonly | _s.oDirectory | _s.oNofollow | _s.oCloexec,
          0,
        ),
      );
    } finally {
      malloc.free(c);
    }
  }

  @override
  OsHandle openDirectoryAt(OsHandle parent, String name) {
    final n = _checkedName(name);
    final c = _cString(n);
    try {
      return _adoptOrThrow(
        _s.openat(
          parent.fd,
          c,
          _s.oRdonly | _s.oDirectory | _s.oNofollow | _s.oCloexec,
          0,
        ),
      );
    } finally {
      malloc.free(c);
    }
  }

  @override
  OsHandle openFileAt(OsHandle parent, String name, {required bool mutable}) {
    final n = _checkedName(name);
    final c = _cString(n);
    final access = mutable ? _s.oRdwr : _s.oRdonly;
    try {
      return _adoptOrThrow(
        _s.openat(parent.fd, c, access | _s.oNofollow | _s.oCloexec, 0),
      );
    } finally {
      malloc.free(c);
    }
  }

  @override
  OsHandle createFileAt(OsHandle parent, String name) {
    final n = _checkedName(name);
    final c = _cString(n);
    try {
      return _adoptOrThrow(
        _s.openat(
          parent.fd,
          c,
          _s.oRdwr | _s.oCreat | _s.oExcl | _s.oNofollow | _s.oCloexec,
          0x180, // 0o600
        ),
      );
    } finally {
      malloc.free(c);
    }
  }

  @override
  void createDirectoryAt(OsHandle parent, String name) {
    final n = _checkedName(name);
    final c = _cString(n);
    try {
      if (_s.mkdirat(parent.fd, c, 0x1ED) != 0) {
        // 0o755
        throw _lastOsError();
      }
    } finally {
      malloc.free(c);
    }
  }

  @override
  void unlinkAt(OsHandle parent, String name, {required bool directory}) {
    final n = _checkedName(name);
    final c = _cString(n);
    try {
      final flags = directory ? _s.atRemovedir : 0;
      if (_s.unlinkat(parent.fd, c, flags) != 0) throw _lastOsError();
    } finally {
      malloc.free(c);
    }
  }

  @override
  void renameAt(
    OsHandle sourceParent,
    String source,
    OsHandle destinationParent,
    String destination, {
    required bool replace,
  }) {
    final src = _checkedName(source);
    final dst = _checkedName(destination);
    final cs = _cString(src);
    final cd = _cString(dst);
    try {
      final result = replace
          ? _s.renameat(sourceParent.fd, cs, destinationParent.fd, cd)
          : _s.renameNoReplaceAt(
              sourceParent.fd,
              cs,
              destinationParent.fd,
              cd,
              _s.renameNoReplace,
            );
      if (result != 0) throw _lastOsError();
    } finally {
      malloc.free(cs);
      malloc.free(cd);
    }
  }

  @override
  List<String> entryNames(OsHandle directory) {
    final duplicated = _s.dup(directory.fd);
    if (duplicated < 0) throw _lastOsError();
    final stream = _s.fdopendir(duplicated);
    if (stream.address == 0) {
      _s.close(duplicated);
      throw _lastOsError();
    }
    final names = <String>[];
    try {
      while (true) {
        _s.errno = 0;
        final entry = _s.readdir(stream);
        if (entry.address == 0) {
          final errno = _s.errno;
          if (errno == 0) break;
          throw IoError.fromErrno(errno);
        }
        final bytes = <int>[];
        for (var i = _s.dNameOffset; ; i++) {
          final byte = (entry + i).value;
          if (byte == 0) break;
          bytes.add(byte);
        }
        final name = utf8.decode(bytes, allowMalformed: true);
        if (name == '.' || name == '..') continue;
        names.add(name);
      }
    } finally {
      _s.closedir(stream);
    }
    return names;
  }

  @override
  HandleStat statHandle(OsHandle handle) {
    final buffer = malloc<Uint8>(256);
    try {
      if (_s.fstat(handle.fd, buffer) != 0) throw _lastOsError();
      return _decodeStat(buffer);
    } finally {
      malloc.free(buffer);
    }
  }

  HandleStat _decodeStat(Pointer<Uint8> buffer) {
    final view = ByteData(256);
    for (var i = 0; i < 256; i++) {
      view.setUint8(i, (buffer + i).value);
    }
    if (_s.isDarwin) {
      return HandleStat(
        dev: view.getInt32(0, Endian.little),
        mode: view.getUint16(4, Endian.little),
        nlink: view.getUint16(6, Endian.little),
        ino: view.getUint64(8, Endian.little),
        uid: view.getUint32(16, Endian.little),
        gid: view.getUint32(20, Endian.little),
        size: view.getInt64(96, Endian.little),
        mtimeNs:
            view.getInt64(48, Endian.little) * 1000000000 +
            view.getInt64(56, Endian.little),
      );
    }
    if (_s.isLinuxArm64) {
      return HandleStat(
        dev: view.getUint64(0, Endian.little),
        ino: view.getUint64(8, Endian.little),
        mode: view.getUint32(16, Endian.little),
        nlink: view.getUint32(20, Endian.little),
        uid: view.getUint32(24, Endian.little),
        gid: view.getUint32(28, Endian.little),
        size: view.getInt64(48, Endian.little),
        mtimeNs:
            view.getInt64(88, Endian.little) * 1000000000 +
            view.getInt64(96, Endian.little),
      );
    }
    // Linux x86_64 struct stat.
    return HandleStat(
      dev: view.getUint64(0, Endian.little),
      ino: view.getUint64(8, Endian.little),
      nlink: view.getUint64(16, Endian.little),
      mode: view.getUint32(24, Endian.little),
      uid: view.getUint32(28, Endian.little),
      gid: view.getUint32(32, Endian.little),
      size: view.getInt64(48, Endian.little),
      mtimeNs:
          view.getInt64(88, Endian.little) * 1000000000 +
          view.getInt64(96, Endian.little),
    );
  }

  @override
  FilesystemObjectIdentity identityOf(OsHandle handle) {
    final stat = statHandle(handle);
    return UnixObjectIdentity(device: stat.dev, inode: stat.ino);
  }

  @override
  int readInto(OsHandle handle, Uint8List buffer) {
    final native = malloc<Uint8>(buffer.length);
    try {
      while (true) {
        final count = _s.read(handle.fd, native, buffer.length);
        if (count < 0) {
          if (_s.errno == _s.eIntr) continue;
          throw _lastOsError();
        }
        buffer.setRange(0, count, native.asTypedList(buffer.length));
        return count;
      }
    } finally {
      malloc.free(native);
    }
  }

  @override
  void writeAll(OsHandle handle, Uint8List bytes) {
    final native = malloc<Uint8>(bytes.isEmpty ? 1 : bytes.length);
    try {
      native.asTypedList(bytes.length).setAll(0, bytes);
      var offset = 0;
      while (offset < bytes.length) {
        final count = _s.write(
          handle.fd,
          (native + offset),
          bytes.length - offset,
        );
        if (count < 0) {
          if (_s.errno == _s.eIntr) continue;
          throw _lastOsError();
        }
        if (count == 0) {
          throw const IoError(FsErrorKind.other, 'failed to write whole buffer');
        }
        offset += count;
      }
    } finally {
      malloc.free(native);
    }
  }

  @override
  void seekTo(OsHandle handle, int position) {
    if (_s.lseek(handle.fd, position, _s.seekSet) < 0) throw _lastOsError();
  }

  @override
  void syncHandle(OsHandle handle) {
    if (_s.fsync(handle.fd) != 0) throw _lastOsError();
  }

  @override
  void truncateHandle(OsHandle handle, int length) {
    if (_s.ftruncate(handle.fd, length) != 0) throw _lastOsError();
  }

  @override
  void chmodHandle(OsHandle handle, int mode) {
    if (_s.fchmod(handle.fd, mode & 0x0FFF) != 0) throw _lastOsError();
  }

  @override
  void chownHandle(OsHandle handle, int uid, int gid) {
    if (_s.fchown(handle.fd, uid, gid) != 0) throw _lastOsError();
  }

  @override
  OsHandle duplicateHandle(OsHandle handle) {
    final fd = _s.dup(handle.fd);
    if (fd < 0) throw _lastOsError();
    return OsHandle.adopt(fd);
  }

  @override
  void closeHandle(OsHandle handle) => handle.close();

  @override
  List<(List<int>, List<int>)> listExtendedAttributes(OsHandle handle) {
    final fd = handle.fd;
    final int listSize;
    if (_s.isDarwin) {
      listSize = _s.flistxattrDarwin!(fd, nullptr, 0, 0);
    } else {
      listSize = _s.flistxattrLinux!(fd, nullptr, 0);
    }
    if (listSize < 0) throw _lastOsError();
    if (listSize > maxSecurityAuxiliaryBytes) {
      throw const IoError(
        FsErrorKind.other,
        'file auxiliary metadata exceeded the size limit',
      );
    }
    final raw = _readXattrNames(fd, listSize);
    raw.sort(_compareBytes);
    final unique = <List<int>>[];
    for (final name in raw) {
      if (unique.isEmpty || _compareBytes(unique.last, name) != 0) {
        unique.add(name);
      }
    }
    if (unique.length > maxSecurityAuxiliaryRecords) {
      throw const IoError(
        FsErrorKind.other,
        'file auxiliary metadata record count exceeded the limit',
      );
    }

    var total = 0;
    final output = <(List<int>, List<int>)>[];
    for (final name in unique) {
      final cName = _cBytes(name);
      final int valueSize;
      try {
        if (_s.isDarwin) {
          valueSize = _s.fgetxattrDarwin!(fd, cName, nullptr, 0, 0, 0);
        } else {
          valueSize = _s.fgetxattrLinux!(fd, cName, nullptr, 0);
        }
        if (valueSize < 0) throw _lastOsError();
        total = _checkedAuxiliaryTotal(total, name.length, valueSize);
        final value = malloc<Uint8>(valueSize == 0 ? 1 : valueSize);
        try {
          if (valueSize > 0) {
            final int read;
            if (_s.isDarwin) {
              read = _s.fgetxattrDarwin!(fd, cName, value, valueSize, 0, 0);
            } else {
              read = _s.fgetxattrLinux!(fd, cName, value, valueSize);
            }
            if (read != valueSize) {
              throw const IoError(
                FsErrorKind.other,
                'file auxiliary metadata changed during capture',
              );
            }
          }
          output.add((
            name,
            List<int>.from(value.asTypedList(valueSize)),
          ));
        } finally {
          malloc.free(value);
        }
      } finally {
        malloc.free(cName);
      }
    }
    return output;
  }

  /// Reads the NUL-separated attribute name list, verifying the size matches.
  List<List<int>> _readXattrNames(int fd, int listSize) {
    final names = malloc<Uint8>(listSize == 0 ? 1 : listSize);
    try {
      if (listSize > 0) {
        final int read;
        if (_s.isDarwin) {
          read = _s.flistxattrDarwin!(fd, names, listSize, 0);
        } else {
          read = _s.flistxattrLinux!(fd, names, listSize);
        }
        if (read != listSize) {
          throw const IoError(
            FsErrorKind.other,
            'file auxiliary metadata changed during capture',
          );
        }
      }
      final bytes = Uint8List.fromList(names.asTypedList(listSize));
      return _splitNames(bytes);
    } finally {
      malloc.free(names);
    }
  }

  int _checkedAuxiliaryTotal(int total, int nameLength, int valueLength) {
    final next = total + nameLength + valueLength + 16;
    if (next > maxSecurityAuxiliaryBytes) {
      throw const IoError(
        FsErrorKind.other,
        'file auxiliary metadata exceeded the size limit',
      );
    }
    return next;
  }

  static List<List<int>> _splitNames(Uint8List bytes) {
    final out = <List<int>>[];
    var start = 0;
    for (var i = 0; i <= bytes.length; i++) {
      if (i == bytes.length || bytes[i] == 0) {
        if (i > start) out.add(bytes.sublist(start, i));
        start = i + 1;
      }
    }
    return out;
  }

  static int _compareBytes(List<int> a, List<int> b) {
    final n = min(a.length, b.length);
    for (var i = 0; i < n; i++) {
      if (a[i] != b[i]) return a[i] - b[i];
    }
    return a.length - b.length;
  }

  static Pointer<Uint8> _cBytes(List<int> bytes) {
    if (bytes.contains(0)) {
      throw const IoError(
        FsErrorKind.other,
        'file auxiliary metadata name is invalid',
      );
    }
    final pointer = malloc<Uint8>(bytes.length + 1);
    pointer.asTypedList(bytes.length + 1)
      ..setAll(0, bytes)
      ..[bytes.length] = 0;
    return pointer;
  }

  @override
  void setExtendedAttribute(OsHandle handle, List<int> name, List<int> value) {
    final cName = _cBytes(name);
    final cValue = malloc<Uint8>(value.isEmpty ? 1 : value.length);
    try {
      cValue.asTypedList(value.length).setAll(0, value);
      final int result;
      if (_s.isDarwin) {
        result = _s.fsetxattrDarwin!(handle.fd, cName, cValue, value.length, 0, 0);
      } else {
        result = _s.fsetxattrLinux!(handle.fd, cName, cValue, value.length, 0);
      }
      if (result != 0) throw _lastOsError();
    } finally {
      malloc.free(cName);
      malloc.free(cValue);
    }
  }

  @override
  int effectiveUid() => _s.geteuid();

  @override
  int effectiveGid() => _s.getegid();

  @override
  OsHandle openPathReadOnly(String path, {required bool noFollow}) {
    final c = _cString(path);
    try {
      final flags = _s.oRdonly | _s.oCloexec | (noFollow ? _s.oNofollow : 0);
      return _adoptOrThrow(_s.open(c, flags, 0));
    } finally {
      malloc.free(c);
    }
  }

  @override
  OsHandle createPathExclusive(String path) {
    final c = _cString(path);
    try {
      return _adoptOrThrow(
        _s.open(
          c,
          _s.oRdwr | _s.oCreat | _s.oExcl | _s.oNofollow | _s.oCloexec,
          0x180,
        ),
      );
    } finally {
      malloc.free(c);
    }
  }

  @override
  void linkPath(String source, String destination) {
    final cs = _cString(source);
    final cd = _cString(destination);
    try {
      if (_s.link(cs, cd) != 0) throw _lastOsError();
    } finally {
      malloc.free(cs);
      malloc.free(cd);
    }
  }

  @override
  void renamePathNoReplace(String source, String destination) {
    final cs = _cString(source);
    final cd = _cString(destination);
    try {
      final result = _s.renameNoReplaceAt(
        _s.atFdcwd,
        cs,
        _s.atFdcwd,
        cd,
        _s.renameNoReplace,
      );
      if (result != 0) throw _lastOsError();
    } finally {
      malloc.free(cs);
      malloc.free(cd);
    }
  }
}

// ---------------------------------------------------------------------------
// Windows implementation: UNVERIFIED. The Rust implementation relies on
// NtCreateFile/NtSetInformationFile with root-relative handles, DuplicateHandle,
// FILE_DISPOSITION_INFO_EX and Zone.Identifier streams. None of it can run on
// this host, so every operation refuses rather than falling back to paths.
// ---------------------------------------------------------------------------

// UNVERIFIED: Windows capability layer requires a Windows host and packaged
// verification (Office/printer acceptance is out of scope for Linux tests).
final class _WindowsCapabilityPlatform implements CapabilityPlatform {
  static Never _unverified(String operation) => throw UnsupportedError(
    'UNVERIFIED Windows capability operation not available on this host: $operation',
  );

  @override
  OsHandle openAmbientDirectory(String path) => _unverified('openAmbientDirectory');
  @override
  OsHandle openDirectoryAt(OsHandle parent, String name) =>
      _unverified('openDirectoryAt');
  @override
  OsHandle openFileAt(OsHandle parent, String name, {required bool mutable}) =>
      _unverified('openFileAt');
  @override
  OsHandle createFileAt(OsHandle parent, String name) => _unverified('createFileAt');
  @override
  void createDirectoryAt(OsHandle parent, String name) =>
      _unverified('createDirectoryAt');
  @override
  void unlinkAt(OsHandle parent, String name, {required bool directory}) =>
      _unverified('unlinkAt');
  @override
  void renameAt(
    OsHandle sourceParent,
    String source,
    OsHandle destinationParent,
    String destination, {
    required bool replace,
  }) => _unverified('renameAt');
  @override
  List<String> entryNames(OsHandle directory) => _unverified('entryNames');
  @override
  HandleStat statHandle(OsHandle handle) => _unverified('statHandle');
  @override
  FilesystemObjectIdentity identityOf(OsHandle handle) => _unverified('identityOf');
  @override
  int readInto(OsHandle handle, Uint8List buffer) => _unverified('readInto');
  @override
  void writeAll(OsHandle handle, Uint8List bytes) => _unverified('writeAll');
  @override
  void seekTo(OsHandle handle, int position) => _unverified('seekTo');
  @override
  void syncHandle(OsHandle handle) => _unverified('syncHandle');
  @override
  void truncateHandle(OsHandle handle, int length) => _unverified('truncateHandle');
  @override
  void chmodHandle(OsHandle handle, int mode) => _unverified('chmodHandle');
  @override
  void chownHandle(OsHandle handle, int uid, int gid) => _unverified('chownHandle');
  @override
  OsHandle duplicateHandle(OsHandle handle) => _unverified('duplicateHandle');
  @override
  void closeHandle(OsHandle handle) => handle.close();
  @override
  List<(List<int>, List<int>)> listExtendedAttributes(OsHandle handle) =>
      _unverified('listExtendedAttributes');
  @override
  void setExtendedAttribute(OsHandle handle, List<int> name, List<int> value) =>
      _unverified('setExtendedAttribute');
  @override
  int effectiveUid() => _unverified('effectiveUid');
  @override
  int effectiveGid() => _unverified('effectiveGid');
  @override
  OsHandle openPathReadOnly(String path, {required bool noFollow}) =>
      _unverified('openPathReadOnly');
  @override
  OsHandle createPathExclusive(String path) => _unverified('createPathExclusive');
  @override
  void linkPath(String source, String destination) => _unverified('linkPath');
  @override
  void renamePathNoReplace(String source, String destination) =>
      _unverified('renamePathNoReplace');
}

// ---------------------------------------------------------------------------
// Small helpers shared by the capability types.
// ---------------------------------------------------------------------------

final Random _random = Random.secure();

/// `Uuid::new_v4()`, hyphenated lowercase.
String newUuidV4() {
  final bytes = List<int>.generate(16, (_) => _random.nextInt(256));
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  final hex = bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
  return '${hex.substring(0, 8)}-${hex.substring(8, 12)}-'
      '${hex.substring(12, 16)}-${hex.substring(16, 20)}-${hex.substring(20)}';
}

/// `Uuid::new_v4().simple()`, 32 lowercase hex digits.
String newUuidV4Simple() => newUuidV4().replaceAll('-', '');

/// Lowercase hex SHA-256 of [bytes], matching `format!("{:x}", digest)`.
String _sha256Hex(List<int> bytes) => crypto.sha256.convert(bytes).toString();

class _DigestCollector implements Sink<crypto.Digest> {
  crypto.Digest? digest;

  @override
  void add(crypto.Digest data) => digest = data;

  @override
  void close() {}
}

/// Streams [handle] from offset 0 and restores the cursor to 0, like
/// `sha256_open_file`.
String sha256OpenFile(OsHandle handle) {
  handle.seek(0);
  final collector = _DigestCollector();
  final sink = crypto.sha256.startChunkedConversion(collector);
  final buffer = Uint8List(64 * 1024);
  while (true) {
    final count = handle.read(buffer);
    if (count == 0) break;
    sink.add(Uint8List.sublistView(buffer, 0, count));
  }
  sink.close();
  handle.seek(0);
  return collector.digest.toString();
}

/// `io::copy`: copies everything from the current position of [input] into
/// [output], returning the number of bytes written.
int copyStream(OsHandle input, OsHandle output) {
  final buffer = Uint8List(64 * 1024);
  var total = 0;
  while (true) {
    final count = input.read(buffer);
    if (count == 0) break;
    output.writeAll(Uint8List.sublistView(buffer, 0, count));
    total += count;
  }
  return total;
}

String _securityDigest(String text) => _sha256Hex(utf8.encode(text));

/// `format!("{:?}", Vec<(Vec<u8>, Vec<u8>)>)`. Digests depend on this exact
/// text, so the formatting is reproduced byte-for-byte.
String _debugAuxiliary(List<(List<int>, List<int>)> records) {
  String bytes(List<int> value) => '[${value.join(', ')}]';
  return '[${records.map((r) => '(${bytes(r.$1)}, ${bytes(r.$2)})').join(', ')}]';
}

bool _auxiliaryEquals(
  List<(List<int>, List<int>)> left,
  List<(List<int>, List<int>)> right,
) {
  if (left.length != right.length) return false;
  for (var i = 0; i < left.length; i++) {
    if (!_bytesEqual(left[i].$1, right[i].$1) ||
        !_bytesEqual(left[i].$2, right[i].$2)) {
      return false;
    }
  }
  return true;
}

bool _bytesEqual(List<int> left, List<int> right) {
  if (left.length != right.length) return false;
  for (var i = 0; i < left.length; i++) {
    if (left[i] != right[i]) return false;
  }
  return true;
}

// ---------------------------------------------------------------------------
// Test hooks (cfg(test) in Rust). Kept public under @visibleForTesting.
// ---------------------------------------------------------------------------

int? _directorySyncFailureCountdown;
bool _postCreateIdentityFailure = false;

// Test-only hook (cfg(test) in Rust).
void injectDirectorySyncFailureAfter(int successfulSyncs) {
  _directorySyncFailureCountdown = successfulSyncs;
}

// Test-only hook (cfg(test) in Rust).
void injectPostCreateIdentityFailureOnce() {
  _postCreateIdentityFailure = true;
}

bool _takePostCreateIdentityFailure() {
  final injected = _postCreateIdentityFailure;
  _postCreateIdentityFailure = false;
  return injected;
}

void _injectedDirectorySyncFailure() {
  final countdown = _directorySyncFailureCountdown;
  if (countdown == null) return;
  if (countdown == 0) {
    _directorySyncFailureCountdown = null;
    throw const IoError(
      FsErrorKind.other,
      'injected post-rename directory sync failure',
    );
  }
  _directorySyncFailureCountdown = countdown - 1;
}

// ---------------------------------------------------------------------------
// Capability types.
// ---------------------------------------------------------------------------

/// A raised effect failure: [outcomeUnknown] is true when the effect may have
/// happened (Rust `CapabilityEffectError`).
class CapabilityEffectError implements Exception {
  const CapabilityEffectError._(this.error, {required this.outcomeUnknown});

  factory CapabilityEffectError.noEffect(IoError error) =>
      CapabilityEffectError._(error, outcomeUnknown: false);

  factory CapabilityEffectError.outcomeUnknown(IoError error) =>
      CapabilityEffectError._(error, outcomeUnknown: true);

  final IoError error;
  final bool outcomeUnknown;

  bool get isOutcomeUnknown => outcomeUnknown;

  IoError get innerError => error;

  @override
  String toString() => 'CapabilityEffectError($error)';
}

T _noEffect<T>(T Function() body) {
  try {
    return body();
  } on IoError catch (error) {
    throw CapabilityEffectError.noEffect(error);
  }
}

T _outcomeUnknown<T>(T Function() body) {
  try {
    return body();
  } on IoError catch (error) {
    throw CapabilityEffectError.outcomeUnknown(error);
  }
}

/// Directory durability barrier (Rust `PlatformDirectory::sync_all`): runs the
/// injected test failure first, then `fsync` on the pinned directory.
void _syncDir(OsHandle directory) {
  _injectedDirectorySyncFailure();
  _platform.syncHandle(directory);
}

/// Opens a handle, applies [use], and closes the handle on every path.
R _withOpen<R>(OsHandle Function() open, R Function(OsHandle) use) {
  final handle = open();
  try {
    return use(handle);
  } finally {
    handle.close();
  }
}

/// Capability over a canonical root directory, opened once with no-follow.
final class CapabilityRoot implements CapabilityRootPort {
  CapabilityRoot._(this.canonical, this._directory, this.access);

  @override
  final String canonical;
  final OsHandle _directory;
  final DirectoryAccess access;

  factory CapabilityRoot.open(String canonical) =>
      CapabilityRoot.openWithAccess(canonical, DirectoryAccess.readWrite);

  factory CapabilityRoot.openWithAccess(
    String canonical,
    DirectoryAccess access,
  ) {
    if (!pathIsAbsolute(canonical)) {
      throw const IoError(
        FsErrorKind.invalidInput,
        'capability root must be absolute',
      );
    }
    return CapabilityRoot._(
      canonical,
      _platform.openAmbientDirectory(canonical),
      access,
    );
  }

  @override
  FilesystemObjectIdentity identity() => _platform.identityOf(_directory);

  @override
  AuthorizedPath authorize(String absolute) =>
      _authorizeInner(absolute, allowRoot: false);

  AuthorizedPath authorizeDirectory(String absolute) =>
      _authorizeInner(absolute, allowRoot: true);

  AuthorizedPath _authorizeInner(String absolute, {required bool allowRoot}) {
    if (!pathIsAbsolute(absolute) ||
        _components(absolute).any((c) => c.kind == _CompKind.parentDir)) {
      throw const IoError(
        FsErrorKind.permissionDenied,
        'path is not capability-relative',
      );
    }
    final relative = pathStripPrefix(absolute, canonical);
    if (relative == null) {
      throw const IoError(
        FsErrorKind.permissionDenied,
        'path escapes capability root',
      );
    }
    if (relative.isEmpty && !allowRoot) {
      throw const IoError(
        FsErrorKind.permissionDenied,
        'capability root itself is not a leaf path',
      );
    }
    _validateRelative(relative);
    return AuthorizedPath._(absolute, relative, this, null);
  }

  DirectoryAccess get rootAccess => access;
}

void _validateRelative(String path) {
  if (pathIsAbsolute(path)) {
    throw const IoError(
      FsErrorKind.permissionDenied,
      'invalid capability-relative path',
    );
  }
  for (final comp in _components(path)) {
    if (comp.kind != _CompKind.normal && comp.kind != _CompKind.curDir) {
      throw const IoError(
        FsErrorKind.permissionDenied,
        'invalid capability-relative path',
      );
    }
  }
}

/// A validated, not-yet-opened path under a [CapabilityRoot].
final class AuthorizedPath implements AuthorizedPathPort {
  AuthorizedPath._(this.absolute, this.relative, this.root, this.expectedIdentity);

  final String absolute;
  final String relative;
  final CapabilityRoot root;
  final FilesystemObjectIdentity? expectedIdentity;

  AuthorizedPath withExpectedIdentity(FilesystemObjectIdentity identity) =>
      AuthorizedPath._(absolute, relative, root, identity);

  /// Opens every ancestor no-follow and pins the parent directory.
  @override
  AnchoredPath anchor() {
    final name = pathFileName(relative);
    if (name == null) {
      throw const IoError(FsErrorKind.invalidInput, 'path has no leaf name');
    }
    final parentPath = pathParent(relative) ?? '';
    var directory = _platform.duplicateHandle(root._directory);
    for (final comp in _components(parentPath)) {
      if (comp.kind != _CompKind.normal) {
        throw const IoError(
          FsErrorKind.permissionDenied,
          'invalid capability-relative ancestor',
        );
      }
      directory = _platform.openDirectoryAt(directory, comp.name);
    }
    return AnchoredPath._(
      absolute,
      directory,
      name,
      expectedIdentity,
      root.access,
    );
  }

  MissingDirectoryCursor missingDirectories() {
    var parent = _platform.duplicateHandle(root._directory);
    var current = root.canonical;
    final comps = _components(relative).toList();
    for (var i = 0; i < comps.length; i++) {
      final comp = comps[i];
      if (comp.kind != _CompKind.normal) {
        throw const IoError(
          FsErrorKind.permissionDenied,
          'invalid capability-relative directory',
        );
      }
      current = pathJoin(current, comp.name);
      try {
        parent = _platform.openDirectoryAt(parent, comp.name);
      } on IoError catch (error) {
        if (error.kind != FsErrorKind.notFound) rethrow;
        final missing = <(String, String)>[(current, comp.name)];
        for (var j = i + 1; j < comps.length; j++) {
          final rest = comps[j];
          if (rest.kind != _CompKind.normal) {
            throw const IoError(
              FsErrorKind.permissionDenied,
              'invalid capability-relative directory',
            );
          }
          current = pathJoin(current, rest.name);
          missing.add((current, rest.name));
        }
        return MissingDirectoryCursor._(parent, missing, root.access);
      }
    }
    return MissingDirectoryCursor._(parent, const [], root.access);
  }

  ArtifactParentCreateOutcome createArtifactParentNoClobber() {
    final AnchoredPath anchored;
    try {
      anchored = anchor();
      anchored.requireWrite();
    } on IoError catch (error) {
      throw CapabilityEffectError.noEffect(error);
    }
    try {
      _platform.createDirectoryAt(anchored.parent, anchored.name);
    } on IoError catch (error) {
      if (error.kind == FsErrorKind.alreadyExists) {
        return const ArtifactParentAlreadyExists();
      }
      throw CapabilityEffectError.noEffect(error);
    }
    try {
      final child = _platform.openDirectoryAt(anchored.parent, anchored.name);
      final identity = _platform.identityOf(child);
      child.close();
      return ArtifactParentCreated(identity);
    } on IoError {
      return const ArtifactParentCreatedIdentityUnknown();
    }
  }

  RetainedStorageUsage s5RetainedStorageUsage(int maxEntries) {
    final directory = _openDirectoryNoFollow();
    var fileCount = 0;
    var logicalCharge = 0;
    for (final name in _platform.entryNames(directory)) {
      if (!s5RetainedPrefixes.any(name.startsWith)) continue;
      fileCount += 1;
      if (fileCount > maxEntries) {
        throw const IoError(
          FsErrorKind.other,
          'retained artifact count exceeded the limit',
        );
      }
      final file = _platform.openFileAt(directory, name, mutable: false);
      final metadata = file.metadata();
      if (!metadata.isFile) {
        throw const IoError(
          FsErrorKind.other,
          'retained artifact is not a regular file',
        );
      }
      final snapshot = FileSecuritySnapshot.capture(file);
      final charge = metadata.size + snapshot.auxiliaryChargeBytes();
      logicalCharge += charge;
    }
    return RetainedStorageUsage(
      fileCount: fileCount,
      logicalChargeBytes: logicalCharge,
    );
  }

  OsHandle _openDirectoryNoFollow() {
    var directory = _platform.duplicateHandle(root._directory);
    for (final comp in _components(relative)) {
      if (comp.kind != _CompKind.normal) {
        throw const IoError(
          FsErrorKind.permissionDenied,
          'invalid capability-relative directory',
        );
      }
      directory = _platform.openDirectoryAt(directory, comp.name);
    }
    return directory;
  }
}

/// Retained-artifact usage scanned from disk (`RetainedStorageUsage`).
class RetainedStorageUsage {
  const RetainedStorageUsage({
    required this.fileCount,
    required this.logicalChargeBytes,
  });

  final int fileCount;
  final int logicalChargeBytes;
}

/// Walks and creates missing directories while pinning each parent handle.
class MissingDirectoryCursor {
  MissingDirectoryCursor._(this._parent, this._missing, this._access);

  OsHandle _parent;
  final List<(String, String)> _missing;
  int _index = 0;
  final DirectoryAccess _access;

  String? get nextPath => _index < _missing.length ? _missing[_index].$1 : null;

  void createNext() {
    if (_access != DirectoryAccess.readWrite) {
      throw CapabilityEffectError.noEffect(
        const IoError(
          FsErrorKind.permissionDenied,
          'read-only capability cannot create a directory',
        ),
      );
    }
    if (_index >= _missing.length) {
      throw CapabilityEffectError.noEffect(
        const IoError(FsErrorKind.other, 'no directory pending'),
      );
    }
    final name = _missing[_index].$2;
    _noEffect(() => _platform.createDirectoryAt(_parent, name));
    final child = _outcomeUnknown(
      () => _platform.openDirectoryAt(_parent, name),
    );
    _outcomeUnknown(() => _syncDir(_parent));
    _parent = child;
    _index += 1;
  }
}

/// Security/access metadata captured from an open file (`FileSecuritySnapshot`).
class FileSecuritySnapshot {
  FileSecuritySnapshot._(this._mode, this._uid, this._gid, this._links, this._auxiliary);

  final int _mode;
  final int _uid;
  final int _gid;
  final int _links;
  final List<(List<int>, List<int>)> _auxiliary;

  static FileSecuritySnapshot capture(OsHandle data) {
    final stat = _platform.statHandle(data);
    if (stat.nlink != 1) {
      throw const IoError(
        FsErrorKind.permissionDenied,
        'secured workbook must have exactly one hard link',
      );
    }
    final auxiliary = _platform.listExtendedAttributes(data);
    return FileSecuritySnapshot._(
      stat.mode,
      stat.uid,
      stat.gid,
      stat.nlink,
      auxiliary,
    );
  }

  String get _debugAccess =>
      'Unix { mode: $_mode, uid: $_uid, gid: $_gid, links: $_links }';

  CreationSecurityDescriptor creationDescriptor(
    ParentSecuritySnapshot parent,
  ) => CreationSecurityDescriptor._(_mode, _uid, _gid);

  void verifyCreatedAccess(SecuredCreatedFile file) {
    final actual = FileSecuritySnapshot.capture(file.handle);
    if (actual._mode != _mode ||
        actual._uid != _uid ||
        actual._gid != _gid ||
        actual._links != _links) {
      throw const IoError(
        FsErrorKind.other,
        'secured file access metadata changed',
      );
    }
  }

  void replayAndVerifyAuxiliary(SecuredCreatedFile file) {
    _replayUnixXattrs(file.handle, _auxiliary);
    final actual = FileSecuritySnapshot.capture(file.handle);
    if (!_auxiliaryEquals(actual._auxiliary, _auxiliary)) {
      throw const IoError(
        FsErrorKind.other,
        'secured file auxiliary metadata changed',
      );
    }
  }

  void verifyComplete(SecuredCreatedFile file) {
    final actual = FileSecuritySnapshot.capture(file.handle);
    if (actual != this) {
      throw const IoError(FsErrorKind.other, 'secured file metadata changed');
    }
  }

  String accessSha256() => _securityDigest(_debugAccess);

  String auxiliarySha256() => _securityDigest(_debugAuxiliary(_auxiliary));

  int auxiliaryChargeBytes() {
    var total = 0;
    for (final (name, value) in _auxiliary) {
      total = _saturatingAdd(total, name.length);
      total = _saturatingAdd(total, value.length);
      total = _saturatingAdd(total, 16);
    }
    return total;
  }

  String sha256() => _securityDigest(
    '$_debugAccess:${_debugAuxiliary(_auxiliary)}',
  );

  @override
  bool operator ==(Object other) =>
      other is FileSecuritySnapshot &&
      other._mode == _mode &&
      other._uid == _uid &&
      other._gid == _gid &&
      other._links == _links &&
      _auxiliaryEquals(other._auxiliary, _auxiliary);

  @override
  int get hashCode => Object.hash(_mode, _uid, _gid, _links, _auxiliary.length);
}

int _saturatingAdd(int a, int b) {
  final sum = a + b;
  return sum < 0 ? 0x7fffffffffffffff : sum;
}

void _replayUnixXattrs(OsHandle file, List<(List<int>, List<int>)> records) {
  if (records.length > maxSecurityAuxiliaryRecords) {
    throw const IoError(
      FsErrorKind.other,
      'file auxiliary metadata record count exceeded the limit',
    );
  }
  for (final (name, value) in records) {
    _platform.setExtendedAttribute(file, name, value);
  }
}

/// Parent directory identity and access bits (`ParentSecuritySnapshot`).
class ParentSecuritySnapshot {
  const ParentSecuritySnapshot._(this.identity, this._mode, this._uid, this._gid);

  final FilesystemObjectIdentity identity;
  final int _mode;
  final int _uid;
  final int _gid;

  @override
  bool operator ==(Object other) =>
      other is ParentSecuritySnapshot &&
      other.identity == identity &&
      other._mode == _mode &&
      other._uid == _uid &&
      other._gid == _gid;

  @override
  int get hashCode => Object.hash(identity, _mode, _uid, _gid);
}

ParentSecuritySnapshot _parentSecuritySnapshot(OsHandle parent) {
  final stat = _platform.statHandle(parent);
  return ParentSecuritySnapshot._(
    _platform.identityOf(parent),
    stat.mode,
    stat.uid,
    stat.gid,
  );
}

/// Owner-only (or copied) creation attributes applied through the owned handle.
/// Rust `CreationSecurityDescriptor` applies access bits only; auxiliary
/// records are replayed separately from the source snapshot.
class CreationSecurityDescriptor {
  const CreationSecurityDescriptor._(this._mode, this._uid, this._gid);

  final int _mode;
  final int _uid;
  final int _gid;

  factory CreationSecurityDescriptor.currentOwnerOnly() {
    return CreationSecurityDescriptor._(
      _Sys.current.sIfReg | 0x180,
      _platform.effectiveUid(),
      _platform.effectiveGid(),
    );
  }
}

/// Owned handle of a file created by [AnchoredPath.createNewSecured].
class SecuredCreatedFile {
  SecuredCreatedFile(this.handle);

  final OsHandle handle;

  void syncAll() => handle.syncAll();

  FilesystemObjectIdentity identity() => _platform.identityOf(handle);

  FileSecuritySnapshot securitySnapshot() =>
      FileSecuritySnapshot.capture(handle);

  int len() => handle.metadata().size;

  String sha256() => sha256OpenFile(handle);
}

sealed class SecuredCreateOutcome {
  const SecuredCreateOutcome();
}

final class SecuredCreated extends SecuredCreateOutcome {
  const SecuredCreated(this.file, this.identity);
  final SecuredCreatedFile file;
  final FilesystemObjectIdentity identity;
}

final class SecuredCreatedIdentityUnknown extends SecuredCreateOutcome {
  const SecuredCreatedIdentityUnknown(this.file);
  final SecuredCreatedFile file;
}

void _applyCreationSecurity(OsHandle file, CreationSecurityDescriptor security) {
  final stat = _platform.statHandle(file);
  if ((stat.uid != security._uid || stat.gid != security._gid)) {
    _platform.chownHandle(file, security._uid, security._gid);
  }
  _platform.chmodHandle(file, security._mode & 0x0FFF);
}

/// A pinned leaf: a parent directory handle plus one validated name.
final class AnchoredPath implements AnchoredPathPort {
  AnchoredPath._(
    this._absolute,
    this.parent,
    this.name,
    this._expectedIdentity,
    this._access,
  );

  final String _absolute;
  final OsHandle parent;
  final String name;
  final FilesystemObjectIdentity? _expectedIdentity;
  final DirectoryAccess _access;

  @override
  String get absolute => _absolute;

  OsHandle openRead() {
    final file = _platform.openFileAt(parent, name, mutable: false);
    _validateExpectedIdentity(file);
    return file;
  }

  OsHandle openMutable() {
    requireWrite();
    final file = _platform.openFileAt(parent, name, mutable: true);
    _validateExpectedIdentity(file);
    return file;
  }

  OsHandle createNew() {
    requireWrite();
    return _platform.createFileAt(parent, name);
  }

  SecuredCreatedFile createNewSecured(CreationSecurityDescriptor security) {
    final file = createNew();
    _applyCreationSecurity(file, security);
    return SecuredCreatedFile(file);
  }

  SecuredCreateOutcome createNewSecuredIdentified(
    CreationSecurityDescriptor security,
  ) {
    _noEffect(requireWrite);
    final raw = _noEffect(() => _platform.createFileAt(parent, name));
    _outcomeUnknown(() => _applyCreationSecurity(raw, security));
    final file = SecuredCreatedFile(raw);
    if (_takePostCreateIdentityFailure()) {
      return SecuredCreatedIdentityUnknown(file);
    }
    try {
      return SecuredCreated(file, file.identity());
    } on IoError {
      return SecuredCreatedIdentityUnknown(file);
    }
  }

  FilesystemObjectIdentity identity() => _withOpen(openRead, _platform.identityOf);

  int len() => _withOpen(openRead, (file) => file.metadata().size);

  bool sameFileAs(AnchoredPath other) {
    final left = openRead();
    final right = other.openRead();
    try {
      return sameFileHandles(left, right);
    } finally {
      left.close();
      right.close();
    }
  }

  @override
  void removeFile() {
    requireWrite();
    _platform.unlinkAt(parent, name, directory: false);
  }

  void removeDir() {
    requireWrite();
    _platform.unlinkAt(parent, name, directory: true);
  }

  @override
  void removeFileDurable() {
    _platform.unlinkAt(parent, name, directory: false);
    _syncDir(parent);
  }

  /// Verifies the content hash on the pinned handle, then removes the name.
  // Rust: #[cfg(any(windows, test))]. Kept on every platform for the restore path.
  @override
  void removeVerified(String expectedSha256) {
    requireWrite();
    final file = openMutable();
    try {
      if (sha256OpenFile(file) != expectedSha256) {
        throw const IoError(
          FsErrorKind.invalidData,
          'file changed before pinned deletion',
        );
      }
    } finally {
      file.close();
    }
    _platform.unlinkAt(parent, name, directory: false);
    _syncDir(parent);
  }

  @override
  void removeDirDurable() {
    _platform.unlinkAt(parent, name, directory: true);
    _syncDir(parent);
  }

  @override
  void copyNoClobberFrom(covariant AnchoredPath source) {
    final input = source.openRead();
    try {
      copyNoClobberFromFile(input);
    } finally {
      input.close();
    }
  }

  void copyNoClobberFromFile(OsHandle input) {
    requireWrite();
    input.seek(0);
    final temp = createTemporary();
    final temporaryName = temp.$1;
    final output = temp.$2;
    try {
      copyStream(input, output);
      _platform.chmodHandle(output, input.metadata().permissionBits);
      output.syncAll();
      output.close();
      _platform.renameAt(
        parent,
        temporaryName,
        parent,
        name,
        replace: false,
      );
      _syncDir(parent);
    } catch (_) {
      output.close();
      try {
        _platform.unlinkAt(parent, temporaryName, directory: false);
      } on IoError {
        // Best-effort cleanup, mirroring `let _ = ...`.
      }
      rethrow;
    }
  }

  void writeNoClobber(List<int> contents) {
    requireWrite();
    final temp = createTemporary();
    final temporaryName = temp.$1;
    final output = temp.$2;
    try {
      output.writeAll(Uint8List.fromList(contents));
      output.syncAll();
      output.close();
      _platform.renameAt(
        parent,
        temporaryName,
        parent,
        name,
        replace: false,
      );
      _syncDir(parent);
    } catch (_) {
      output.close();
      try {
        _platform.unlinkAt(parent, temporaryName, directory: false);
      } on IoError {
        // Best-effort cleanup.
      }
      rethrow;
    }
  }

  @override
  void moveNoClobberTo(covariant AnchoredPath destination) {
    requireWrite();
    destination.requireWrite();
    try {
      _platform.renameAt(
        parent,
        name,
        destination.parent,
        destination.name,
        replace: false,
      );
      _syncDir(destination.parent);
      _syncDir(parent);
    } on IoError catch (error) {
      if (error.osError != _Sys.current.eXdev) rethrow;
      destination.copyNoClobberFrom(this);
      removeFile();
      _syncDir(parent);
    }
  }

  /// Verifies the current handle hash before replacing bytes in place.
  // Rust: #[cfg(any(windows, test))].
  @override
  void overwriteVerifiedFrom(
    covariant AnchoredPath source,
    String expectedCurrentSha256,
  ) {
    requireWrite();
    final input = source.openRead();
    final sourceSha256 = sha256OpenFile(input);
    final sourceMode = input.metadata().permissionBits;
    final output = openMutable();
    try {
      if (sha256OpenFile(output) != expectedCurrentSha256) {
        throw const IoError(
          FsErrorKind.invalidData,
          'destination changed before pinned restore',
        );
      }
      input.seek(0);
      output.seek(0);
      output.setLen(0);
      copyStream(input, output);
      output.setPermissionBits(sourceMode);
      output.syncAll();
      if (sha256OpenFile(output) != sourceSha256) {
        throw const IoError(
          FsErrorKind.other,
          'pinned restore verification failed',
        );
      }
      _syncDir(parent);
    } finally {
      output.close();
      input.close();
    }
  }

  void retainCurrentNameNoClobber(AnchoredPath retained) {
    _noEffect(requireWrite);
    _noEffect(retained.requireWrite);
    final sourceParent = _noEffect(() => _platform.identityOf(parent));
    final retainedParent = _noEffect(() => _platform.identityOf(retained.parent));
    if (sourceParent != retainedParent) {
      throw CapabilityEffectError.noEffect(
        const IoError(
          FsErrorKind.invalidInput,
          'retained publication must remain in the canonical parent',
        ),
      );
    }
    _noEffect(
      () => _platform.renameAt(
        parent,
        name,
        retained.parent,
        retained.name,
        replace: false,
      ),
    );
    _outcomeUnknown(() => _syncDir(parent));
  }

  void restoreOriginalNameNoClobber(AnchoredPath canonical) {
    _noEffect(requireWrite);
    _noEffect(canonical.requireWrite);
    final retainedParent = _noEffect(() => _platform.identityOf(parent));
    final canonicalParent = _noEffect(() => _platform.identityOf(canonical.parent));
    if (retainedParent != canonicalParent) {
      throw CapabilityEffectError.noEffect(
        const IoError(
          FsErrorKind.invalidInput,
          'original restoration must remain in the canonical parent',
        ),
      );
    }
    _noEffect(
      () => _platform.renameAt(
        parent,
        name,
        canonical.parent,
        canonical.name,
        replace: false,
      ),
    );
    _outcomeUnknown(() => _syncDir(parent));
  }

  void syncParent() {
    requireWrite();
    _syncDir(parent);
  }

  ParentSecuritySnapshot captureParentSecurity() =>
      _parentSecuritySnapshot(parent);

  void verifyParentSecurity(ParentSecuritySnapshot expected) {
    final actual = captureParentSecurity();
    if (actual != expected) {
      throw const IoError(
        FsErrorKind.other,
        'capability parent security changed',
      );
    }
  }

  @override
  void replaceWith(covariant AnchoredPath source) {
    requireWrite();
    source.requireWrite();
    _platform.renameAt(
      source.parent,
      source.name,
      parent,
      name,
      replace: true,
    );
    _syncDir(parent);
  }

  @override
  bool canOpenRead() {
    try {
      openRead().close();
      return true;
    } on IoError {
      return false;
    }
  }

  @override
  String sha256() {
    final file = openRead();
    try {
      return sha256OpenFile(file);
    } finally {
      file.close();
    }
  }

  @override
  AnchoredPath temporarySibling(String label) {
    final temporary = '.$name.docflow-$label-${newUuidV4()}.tmp';
    return AnchoredPath._(
      pathJoin(pathParent(_absolute) ?? '', temporary),
      _platform.duplicateHandle(parent),
      temporary,
      null,
      _access,
    );
  }

  void _validateExpectedIdentity(OsHandle file) {
    final expected = _expectedIdentity;
    if (expected == null) return;
    if (_platform.identityOf(file) != expected) {
      throw const IoError(
        FsErrorKind.permissionDenied,
        'capability leaf identity changed',
      );
    }
  }

  void requireWrite() {
    if (_access != DirectoryAccess.readWrite) {
      throw const IoError(
        FsErrorKind.permissionDenied,
        'read-only capability cannot mutate the filesystem',
      );
    }
  }

  (String, OsHandle) createTemporary() {
    requireWrite();
    for (var attempt = 0; attempt < 16; attempt++) {
      final temporary = '.$name.docflow-${newUuidV4()}.tmp';
      try {
        return (temporary, _platform.createFileAt(parent, temporary));
      } on IoError catch (error) {
        if (error.kind == FsErrorKind.alreadyExists) continue;
        rethrow;
      }
    }
    throw const IoError(
      FsErrorKind.alreadyExists,
      'could not allocate a capability-relative temporary file',
    );
  }
}

/// Identity of a file handle (`file_identity`).
FilesystemObjectIdentity fileIdentity(OsHandle file) =>
    _platform.identityOf(file);

/// Same-object check for two open handles (`same_file`).
bool sameFileHandles(OsHandle left, OsHandle right) {
  final a = left.metadata();
  final b = right.metadata();
  return a.dev == b.dev && a.ino == b.ino;
}

/// Exposed for fs_ops/leaf_transaction which need raw handle operations.
CapabilityPlatform get capabilityPlatform => _platform;

/// Opens a read-only path (`open_source_no_follow`), no-follow on POSIX.
OsHandle openSourceNoFollow(String path) =>
    _platform.openPathReadOnly(path, noFollow: true);

/// Opens a directory for durable sync (`sync_directory`), following links
/// like `File::open` does.
void syncDirectoryPath(String path) {
  final handle = _platform.openPathReadOnly(path, noFollow: false);
  try {
    handle.syncAll();
  } finally {
    handle.close();
  }
}

/// Creates a new file for [path] with `O_EXCL` (no-clobber create).
OsHandle createExclusivePath(String path) => _platform.createPathExclusive(path);

/// Runs the injected directory-sync failure check (Rust `injected_directory_sync_failure`).
/// Called by the directory sync path before `fsync`.
void injectedDirectorySyncFailureCheck() => _injectedDirectorySyncFailure();

/// `std::fs::remove_file` on a path, mapped to [IoError].
void removePathFile(String path) {
  try {
    File(path).deleteSync();
  } on FileSystemException catch (error) {
    final code = error.osError?.errorCode;
    if (code == null || code == 0) {
      throw IoError(FsErrorKind.other, error.message);
    }
    throw IoError.fromErrno(code);
  }
}

/// Creates a secure temporary workspace removed on [dispose] (`tempfile`).
class TempWorkspace {
  TempWorkspace._(this.path);

  final String path;

  static TempWorkspace create() {
    final dir = Directory.systemTemp.createTempSync('docflow-');
    return TempWorkspace._(dir.path);
  }

  void dispose() {
    final dir = Directory(path);
    if (dir.existsSync()) dir.deleteSync(recursive: true);
  }
}

/// Logical storage reservation for one retained-publication step
/// (`RetainedStorageReservation`).
class RetainedStorageReservation {
  const RetainedStorageReservation._({
    required this.fileCount,
    required this.logicalChargeBytes,
  });

  final int fileCount;
  final int logicalChargeBytes;

  /// Apply keeps the original, the private output, and the backup: three files.
  static RetainedStorageReservation forApply(
    int originalDefaultStreamBytes,
    int originalAuxiliaryChargeBytes,
  ) {
    final base = _checkedMul(originalDefaultStreamBytes, 2);
    final withPackage = base == null ? null : _checkedAdd(base, maxPackageBytes);
    final auxiliary = _checkedMul(originalAuxiliaryChargeBytes, 3);
    final total = withPackage == null || auxiliary == null
        ? null
        : _checkedAdd(withPackage, auxiliary);
    if (total == null) {
      throw const FsIoException(
        FsErrorKind.other,
        'retained storage reservation overflowed',
      );
    }
    return RetainedStorageReservation._(
      fileCount: 3,
      logicalChargeBytes: total,
    );
  }

  /// Undo retains the applied workbook: one file.
  static RetainedStorageReservation forUndo(
    int currentAppliedDefaultStreamBytes,
    int currentAppliedAuxiliaryChargeBytes,
  ) {
    final total = _checkedAdd(
      currentAppliedDefaultStreamBytes,
      currentAppliedAuxiliaryChargeBytes,
    );
    if (total == null) {
      throw const FsIoException(
        FsErrorKind.other,
        'retained storage reservation overflowed',
      );
    }
    return RetainedStorageReservation._(
      fileCount: 1,
      logicalChargeBytes: total,
    );
  }
}

int? _checkedMul(int left, int right) {
  if (left < 0 || right < 0) return null;
  if (right != 0 && left > 0x7fffffffffffffff ~/ right) return null;
  return left * right;
}

int? _checkedAdd(int left, int right) {
  if (left < 0 || right < 0) return null;
  if (left > 0x7fffffffffffffff - right) return null;
  return left + right;
}
