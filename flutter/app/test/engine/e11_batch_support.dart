// Fixtures for the Rust `src-tauri/tests/batch_print.rs` port: the fake
// PrintAdapter (outcome queue, submit/inventory hooks in place of Rust's
// thread gates), the PRIVATE-CELL workbook writer, and PrintService wiring.
import 'dart:io';
import 'dart:typed_data';

import 'package:archive/archive.dart';
import 'package:docflow/engine/ooxml.dart';
import 'package:docflow/engine/paths.dart';
import 'package:docflow/engine/print_queue.dart';
import 'package:docflow/engine/progress.dart';
import 'package:path/path.dart' as p;

const batchCaller = CallerIdentity('operator', 'session');

/// Rust `FakeAdapter`. [outcomes] is consumed one per submit (null = success).
/// [onSubmit] and [onInventory] run synchronously where Rust blocks on a gate.
final class FakeBatchAdapter implements PrintAdapter {
  FakeBatchAdapter({
    this.printers = const ['Office Printer'],
    this.defaultPrinter = 'Office Printer',
  });

  final List<String> printers;
  final String? defaultPrinter;
  final List<PrintFailure?> outcomes = [];
  final List<PrintJob> submitted = [];
  final List<PrintSettings> inspectedSettings = [];
  int inspected = 0;
  int? pdfPageCount;
  String? unsupportedPaper;
  void Function(PrintJob job)? onSubmit;
  void Function()? onInventory;
  void Function(String path)? onInspect;
  final List<List<int>> submittedBytes = [];

  void push(PrintFailure? outcome) => outcomes.add(outcome);

  @override
  PrinterInventory inventory() {
    onInventory?.call();
    return PrinterInventory(
      printers: printers,
      defaultPrinter: defaultPrinter,
    ).normalized();
  }

  @override
  PrintJobSupport inspect(String path, String printer, PrintSettings settings) {
    inspected += 1;
    inspectedSettings.add(settings);
    onInspect?.call(path);
    if (unsupportedPaper == settings.paper) {
      throw PrintFailure.unsupportedOptions(
        'paper is unsupported by the test printer',
      );
    }
    switch (p.extension(path).toLowerCase()) {
      case '.pdf':
        return PrintJobSupport.controlledPdf(pageCount: pdfPageCount);
      case '.png' || '.jpg' || '.jpeg' || '.bmp' || '.tif' || '.tiff':
        return PrintJobSupport.singlePage();
      case '.docx':
        return PrintJobSupport.nativeApplication();
      case '.xlsx' || '.xlsm':
        return PrintJobSupport.controlled();
      default:
        throw PrintFailure.unsupported('unsupported test format');
    }
  }

  @override
  void submit(PrintJob job, bool Function() cancelled) {
    if (cancelled()) {
      throw PrintFailure.cancelled('cancelled by test', uncertain: false);
    }
    submitted.add(job);
    final file = File(job.path);
    submittedBytes.add(file.existsSync() ? file.readAsBytesSync() : const []);
    onSubmit?.call(job);
    final outcome = outcomes.isEmpty ? null : outcomes.removeAt(0);
    if (outcome != null) {
      throw outcome;
    }
  }
}

/// PrintService wired as the Rust `print_service(root, adapter)` helper.
PrintService batchService(Directory root, PrintAdapter adapter) => PrintService(
  queueRoot: p.join(root.path, 'print-queues'),
  journalRoot: p.join(root.path, 'journal'),
  ttl: const Duration(minutes: 5),
  mutationLock: GlobalMutationLock(),
  adapter: adapter,
);

/// Rust `params(files)`.
BatchPrintParams batchParams(List<String> files) => BatchPrintParams(
  files: files,
  printer: 'Office Printer',
  paper: 'A4',
  orientation: 'auto',
  scaling: 'fit',
  color: 'printer',
  duplex: 'printer',
  copies: 1,
  collate: true,
  retries: 0,
  onError: 'pause',
  timeoutMinutes: 30,
);

/// One worksheet entry of a Rust-shaped workbook: name, sheet id, visibility.
typedef WorkbookSheet = (String name, int sheetId, String? state);

/// Port of the Rust `write_workbook_with_relationships` helper. Every sheet
/// part holds `PRIVATE CELL VALUE` so tests can prove it never leaks.
void writeWorkbook(
  String path,
  List<WorkbookSheet> sheets, {
  String workbookExtra = '',
  String extraRelationships = '',
  Map<String, List<int>> extra = const {},
}) {
  const main = 'http://schemas.openxmlformats.org/spreadsheetml/2006/main';
  const rel =
      'http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet';
  final sheetElements = StringBuffer();
  final relationshipElements = StringBuffer();
  for (var index = 0; index < sheets.length; index++) {
    final (name, sheetId, state) = sheets[index];
    final stateAttr = state == null ? '' : ' state="$state"';
    sheetElements.write(
      '<sheet name="$name" sheetId="$sheetId"$stateAttr r:id="rId${index + 1}"/>',
    );
    relationshipElements.write(
      '<Relationship Id="rId${index + 1}" Type="$rel" '
      'Target="worksheets/sheet${index + 1}.xml"/>',
    );
  }
  final workbook =
      '<workbook xmlns="$main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
      '<sheets>$sheetElements</sheets>$workbookExtra</workbook>';
  final relationships =
      '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
      '$relationshipElements$extraRelationships</Relationships>';
  const packageRelationships =
      '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
      '<Relationship Id="rIdWorkbook" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>'
      '</Relationships>';
  final archive = Archive();
  void add(String name, List<int> bytes) => archive.addFile(
    ArchiveFile(name, bytes.length, Uint8List.fromList(bytes)),
  );
  add('_rels/.rels', packageRelationships.codeUnits);
  add('xl/workbook.xml', workbook.codeUnits);
  add('xl/_rels/workbook.xml.rels', relationships.codeUnits);
  for (var index = 0; index < sheets.length; index++) {
    add(
      'xl/worksheets/sheet${index + 1}.xml',
      '<worksheet><sheetData><v>PRIVATE CELL VALUE</v></sheetData></worksheet>'
          .codeUnits,
    );
  }
  for (final entry in extra.entries) {
    add(entry.key, entry.value);
  }
  File(path).writeAsBytesSync(ZipEncoder().encode(archive)!);
}

/// Rust `safe_workbook_facts()`.
WorkbookSafetyFacts safeWorkbookFacts() => const WorkbookSafetyFacts();

/// Plain temporary directory removed after the test.
Directory batchTempDir(String name) {
  final dir = Directory.systemTemp.createTempSync('e11-$name-');
  return dir;
}
