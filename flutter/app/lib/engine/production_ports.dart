// Production adapters that bind the engine's authority ports (fs_ops.dart,
// cloud.dart, outlook_reply_draft.dart) to the concrete Dart modules. In Rust
// these were direct calls from fs_ops.rs; Dart keeps the ports abstract.

import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'cloud.dart';
import 'contracts.dart';
import 'fs_ops.dart';
import 'journal.dart';
import 'ooxml.dart' as ox;
import 'operations/replica_publish.dart' as replica;
import 'pdf.dart' as pdf;
import 'rfi_follow_up.dart';
import 'rfi_log.dart' as rfi_log;
import 'undo.dart' as undo;
import 'platform/windows/outlook.dart';

/// `FollowUpOoxml` over the E6 OOXML layer.
final class ProductionFollowUpOoxml implements FollowUpOoxml {
  const ProductionFollowUpOoxml();

  T _guard<T>(T Function() body) {
    try {
      return body();
    } on ox.OoxmlError catch (error) {
      throw FollowUpOoxmlError(
        error.kind == ox.OoxmlErrorKind.limitExceeded,
        error.message,
      );
    }
  }

  @override
  FollowUpParsedWorkbook readWorkbookAndDiscovery(Uint8List bytes) => _guard(() {
    final (discovery, data) = ox.readWorkbookAndDiscovery(bytes);
    return FollowUpParsedWorkbook(discovery, data);
  });

  @override
  bool packageHasSignature(Uint8List bytes) =>
      _guard(() => ox.packageHasSignature(bytes));

  @override
  Uint8List patchWorkbookSheetsGuardedText(
    Uint8List source,
    Map<String, Map<(int, int), ox.GuardedCellUpdate>> updates,
  ) => _guard(() => ox.patchWorkbookSheetsGuardedText(source, updates));

  @override
  void verifyGuardedWorkbookDeltaText(
    Uint8List original,
    Uint8List output,
    Map<String, Map<(int, int), ox.GuardedCellUpdate>> updates,
  ) => _guard(
    () => ox.verifyGuardedWorkbookDeltaText(original, output, updates),
  );
}

/// `RfiLogWorkbookPort` over `rfi_log.dart`. Verification re-reads a file, so
/// the bytes go through a private temp file.
final class ProductionRfiLogWorkbook implements RfiLogWorkbookPort {
  const ProductionRfiLogWorkbook();

  @override
  Map<String, Map<(int, int), ox.CellUpdate>> workbookStepUpdates(Step step) =>
      rfi_log.workbookStepUpdates(step);

  @override
  int workbookRecordCount(Step step) => rfi_log.workbookRecordCount(step);

  @override
  void verifyWorkbookStep(Step step, Uint8List output) {
    final dir = Directory.systemTemp.createTempSync('docflow-verify-');
    try {
      final file = File('${dir.path}/workbook.xlsx')..writeAsBytesSync(output);
      rfi_log.verifyWorkbookStep(step, file.path);
    } finally {
      dir.deleteSync(recursive: true);
    }
  }
}

/// `PdfEngine` over `pdf.dart`, which works on paths.
final class ProductionPdfEngine implements PdfEngine {
  const ProductionPdfEngine();

  Uint8List _run(
    List<Uint8List> inputs,
    void Function(List<String> inputs, String output) body,
  ) {
    final dir = Directory.systemTemp.createTempSync('docflow-pdf-');
    try {
      final paths = <String>[];
      for (var index = 0; index < inputs.length; index++) {
        final file = File('${dir.path}/in$index.pdf')
          ..writeAsBytesSync(inputs[index]);
        paths.add(file.path);
      }
      final output = '${dir.path}/out.pdf';
      body(paths, output);
      return File(output).readAsBytesSync();
    } finally {
      dir.deleteSync(recursive: true);
    }
  }

  @override
  Uint8List replaceFirstPageIncrementalFromPage(
    Uint8List archive,
    Uint8List signedPdf,
    int signedPageIndex,
  ) => _run(
    [archive, signedPdf],
    (inputs, output) => pdf.replaceFirstPageIncrementalFromPage(
      inputs[0],
      inputs[1],
      signedPageIndex,
      output,
    ),
  );

  @override
  Uint8List rotatePagesIncremental(
    Uint8List source,
    List<(int, int)> rotations,
  ) => _run(
    [source],
    (inputs, output) =>
        pdf.rotatePagesIncremental(inputs[0], output, rotations),
  );

  @override
  Uint8List extractPages(Uint8List source, List<int> pageIndexes) => _run(
    [source],
    (inputs, output) => pdf.extractPages(inputs[0], pageIndexes, output),
  );
}

/// `ReplicaPublishPort` over `operations/replica_publish.dart`.
final class ProductionReplicaPublish implements ReplicaPublishPort {
  const ProductionReplicaPublish();

  @override
  ReplicaPublishStepMeta decodeStepMeta(Step step) {
    final meta = replica.decodeReplicaStepMeta(step);
    return ReplicaPublishStepMeta(
      backupPath: meta.backupPath,
      sourceSha256: meta.sourceSha256,
      expectedTargetSha256: meta.expectedTargetSha256,
    );
  }
}

/// Cloud history's undoable flag: `journal_is_safely_undoable` on the entry.
final class JournalUndoPolicy implements LocalJournalUndoPolicy {
  const JournalUndoPolicy();

  @override
  bool isSafelyUndoable(Map<String, Object?> entry) {
    try {
      final record = JournalRecord.fromJson(
        jsonDecode(jsonEncode(entry)) as Map<String, Object?>,
      );
      return undo.journalIsSafelyUndoable(record);
    } on Object {
      return false;
    }
  }
}

/// `UnsupportedOutlookAdapter`: every non-Windows host.
final class UnsupportedOutlookAdapter implements OutlookAdapter {
  const UnsupportedOutlookAdapter();

  Never _unsupported() => throw const OutlookFailure.unsupported(
    'error.rfi.outlookDraft.unsupportedPlatform',
  );

  @override
  SelectedMailItem selectedMailItem() => _unsupported();

  @override
  OutlookDraft replyAll(MailItemIdentity expected) => _unsupported();

  @override
  void deleteUnchangedDraft(DraftIdentity draft) => _unsupported();

  @override
  Uint8List exportSelectedMsg(
    MailItemIdentity expected,
    String expectedFingerprint,
  ) => _unsupported();
}
