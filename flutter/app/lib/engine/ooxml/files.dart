part of '../ooxml.dart';

// File-level entry points. Port of the path-based wrappers in ooxml.rs.
// Output is created exclusively (never clobbered) and only after the patch
// has been computed, so a rejected input leaves no output behind. The
// created handle stays bound to its inode even if the parent path is later
// swapped, which is the property the Rust pinned-handle writer provides.

/// Reads a source package from disk, enforcing the package size limit.
Uint8List readSourcePackage(String path) {
  final file = File(path);
  if (file.lengthSync() > maxPackageBytes) {
    throw const OoxmlError.limitExceeded(
      'workbook package exceeded the size limit',
    );
  }
  return file.readAsBytesSync();
}

/// Creates [path] exclusively and opens it for reading and writing. Throws a
/// [FileSystemException] when the path already exists.
RandomAccessFile createNewOutputFile(String path) {
  final file = File(path);
  file.createSync(exclusive: true);
  return file.openSync(mode: FileMode.write);
}

/// Replaces the content behind [handle] with [bytes].
void writeOutputHandle(RandomAccessFile handle, Uint8List bytes) {
  handle.setPositionSync(0);
  handle.truncateSync(0);
  handle.writeFromSync(bytes);
  handle.flushSync();
}

/// Reads the whole content behind [handle].
Uint8List readOutputHandle(RandomAccessFile handle) {
  handle.setPositionSync(0);
  return handle.readSync(handle.lengthSync());
}

/// Port of patch_workbook_sheets (path form): source from [sourcePath], output
/// created exclusively at [destinationPath].
void patchWorkbookSheetsFile(
  String sourcePath,
  String destinationPath,
  Map<String, Map<(int, int), CellUpdate>> updatesBySheet, {
  int? allowConsecutiveDuplicatesColumn,
}) {
  final patched = patchWorkbookSheets(
    readSourcePackage(sourcePath),
    updatesBySheet,
    allowConsecutiveDuplicatesColumn: allowConsecutiveDuplicatesColumn,
  );
  final handle = createNewOutputFile(destinationPath);
  try {
    writeOutputHandle(handle, patched);
  } finally {
    handle.closeSync();
  }
}

/// Port of patch_workbook_sheets_guarded writing into an already-open
/// [destination] handle (the Rust writer takes an open destination too).
void patchWorkbookSheetsGuardedToHandle(
  String sourcePath,
  RandomAccessFile destination,
  Map<String, Map<(int, int), GuardedCellUpdate>> updatesBySheet,
) {
  final patched = patchWorkbookSheetsGuarded(
    readSourcePackage(sourcePath),
    updatesBySheet,
  );
  writeOutputHandle(destination, patched);
}
