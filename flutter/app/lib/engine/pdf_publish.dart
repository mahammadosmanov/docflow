// Port of src-tauri/src/pdf_publish.rs.
//
// PDF publication without clobbering: a prepared temporary PDF is hard-linked
// to its destination (which fails when the destination exists), and only when
// the link is unsupported or blocked by access errors is it copied exclusively
// with O_EXCL. A destination is never overwritten.

import 'dart:io';

import 'cap_fs.dart'
    show
        IoError,
        OsHandle,
        capabilityPlatform,
        copyStream,
        createExclusivePath,
        fileIdentity,
        openSourceNoFollow,
        removePathFile;
import 'journal.dart' show FsErrorKind, FsIoException;

/// `EXDEV` (cross-device link) is 18 on Linux and Darwin.
const int _exdev = 18;

/// Windows/Linux/Darwin error codes that mean the temporary file is held open
/// or locked by another process (ERROR_ACCESS_DENIED, EACCES, EIO, EBUSY,
/// ETXTBSY) and are worth retrying.
const Set<int> _temporaryAccessErrnos = {5, 13, 16, 32, 33};

/// Signature of the no-clobber link primitive (`L: FnMut(&Path, &Path)`).
typedef PdfLink = void Function(String temporary, String destination);

/// Signature of the wait between retries (`S: FnMut(Duration)`).
typedef PdfWait = void Function(Duration duration);

/// `publish_pdf_no_clobber_with`. [link] defaults to the capability hard-link
/// primitive and [wait] to a real sleep; tests inject both.
void publishPdfNoClobberWith({
  required String temporary,
  required String destination,
  required int attempts,
  PdfLink? link,
  PdfWait? wait,
}) {
  final linkFn = link ?? _defaultLink;
  final waitFn = wait ?? sleep;
  final total = attempts < 1 ? 1 : attempts;
  for (var attempt = 0; attempt < total; attempt++) {
    try {
      linkFn(temporary, destination);
      removeWithAccessRetry(temporary, 40);
      return;
    } on FsIoException catch (error) {
      if (error.kind == FsErrorKind.alreadyExists) rethrow;
      if (_destinationExists(destination)) {
        throw IoError(
          FsErrorKind.alreadyExists,
          'destination already exists: $error',
        );
      }
      if (_isTemporaryPdfAccessError(error) && attempt + 1 < total) {
        waitFn(const Duration(milliseconds: 150));
        continue;
      }
      if (_isTemporaryPdfAccessError(error) ||
          error.kind == FsErrorKind.unsupported ||
          _osErrorOf(error) == _exdev ||
          error.kind == FsErrorKind.permissionDenied) {
        break;
      }
      rethrow;
    }
  }
  copyPreparedPdfExclusive(temporary, destination);
}

/// `remove_file_with_access_retry`: removes [path], treating NotFound as
/// success and retrying access errors with a 150 ms wait.
void removeFileWithAccessRetry(String path, int attempts) {
  for (var attempt = 0; attempt < attempts; attempt++) {
    try {
      removePathFile(path);
      return;
    } on IoError catch (error) {
      if (error.kind == FsErrorKind.notFound) return;
      if (_isTemporaryPdfAccessError(error) && attempt + 1 < attempts) {
        sleep(const Duration(milliseconds: 150));
        continue;
      }
      rethrow;
    }
  }
  throw const FsIoException(
    FsErrorKind.other,
    'temporary PDF could not be removed',
  );
}

/// `temporary_pdf_name_matches`: whether [candidate] is a temporary name that
/// belongs to [destination]'s publication (`.{dest}.docflow-*.tmp`).
bool temporaryPdfNameMatches(String candidate, String destination) =>
    candidate.startsWith('.$destination.docflow-') &&
    candidate.endsWith('.tmp');

/// `same_file_identity`: true when both paths name the same object. Uses
/// no-follow opens, so a symlink never matches its target.
// UNVERIFIED: Rust compares symlink_metadata (the link itself) on unix; this
// port opens with O_NOFOLLOW, so symlinks compare as not-the-same. The
// Windows identity path is unimplemented on this platform port.
bool sameFileIdentity(String left, String right) {
  try {
    final first = openSourceNoFollow(left);
    try {
      final second = openSourceNoFollow(right);
      try {
        return fileIdentity(first) == fileIdentity(second);
      } finally {
        second.close();
      }
    } finally {
      first.close();
    }
  } on Object {
    return false;
  }
}

/// `copy_prepared_pdf_exclusive`: copies [temporary] into a fresh [destination]
/// with O_EXCL, syncs it, then removes the temporary. A failed copy removes
/// only the destination it created.
void copyPreparedPdfExclusive(String temporary, String destination) {
  final input = openSourceNoFollow(temporary);
  final OsHandle output;
  try {
    output = createExclusivePath(destination);
  } catch (_) {
    input.close();
    rethrow;
  }
  try {
    copyStream(input, output);
    output.syncAll();
  } catch (_) {
    output.close();
    input.close();
    try {
      removePathFile(destination);
    } on Object {
      // Best effort: the original copy error is the one that matters.
    }
    rethrow;
  }
  output.close();
  input.close();
  removeWithAccessRetry(temporary, 40);
}

bool removeWithAccessRetry(String path, int attempts) {
  try {
    removeFileWithAccessRetry(path, attempts);
    return true;
  } on Object {
    return false;
  }
}

void _defaultLink(String temporary, String destination) =>
    capabilityPlatform.linkPath(temporary, destination);

bool _destinationExists(String destination) =>
    FileSystemEntity.typeSync(destination) != FileSystemEntityType.notFound;

int? _osErrorOf(Object error) => error is IoError ? error.osError : null;

bool _isTemporaryPdfAccessError(Object error) {
  if (error is FsIoException && error.kind == FsErrorKind.permissionDenied) {
    return true;
  }
  final code = _osErrorOf(error);
  return code != null && _temporaryAccessErrnos.contains(code);
}
