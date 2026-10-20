// Port of the inline `mod tests` in src-tauri/src/rfi_follow_up.rs: candidate
// padding, compound RFI parser, status policy, Worley marker, projection
// budget, authority decoding, plan construction, source analysis count and
// merge budget.
import 'dart:typed_data';

import 'package:docflow/engine/contracts.dart';
import 'package:docflow/engine/ooxml.dart';
import 'package:docflow/engine/rfi_follow_up.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e8_support.dart';

WorksheetDescriptor _descriptor() => const WorksheetDescriptor(
  workbookIndex: 1,
  sheetId: 7,
  relationshipId: 'rId7',
  relationshipType: 'worksheet',
  partPath: 'xl/worksheets/sheet7.xml',
  name: 'Follow Up',
  visibility: WorksheetVisibility.visible,
);

FollowUpFingerprint _fingerprint() => FollowUpFingerprint(
  identity: const FollowUpFileIdentity.unix(device: 1, inode: 2),
  size: 3,
  mtimeNs: 4,
  sha256: '00' * 32,
);

FollowUpApprovedUpdate _approvedUpdate() => FollowUpApprovedUpdate(
  rowId: 'rfi-follow-up:7:2:2',
  canonicalRfi: 'AZE-SUN-INT-QMS-RFI-CIV-0001',
  proposedStatus: 'Signed',
  proposedRfi: 'CIV-0001',
  target: FollowUpCellTarget(
    worksheet: _descriptor(),
    row: 2,
    rfiColumn: 1,
    statusColumn: 2,
    rfiCell: const CellData(value: 'CIV-0001'),
    statusCell: const CellData(),
  ),
  sourceEvidence: const [],
);

FollowUpStepMeta _stepMeta(List<FollowUpApprovedUpdate> updates) =>
    FollowUpStepMeta(
      rfiLogWorkbook: '/source.xlsx',
      rfiLogFingerprint: _fingerprint(),
      followUpFingerprint: _fingerprint(),
      layout: FollowUpLayout(
        worksheet: _descriptor(),
        headerRow: 1,
        rfiColumn: 1,
        statusColumn: 2,
      ),
      layouts: const [],
      updates: updates,
    );

void main() {
  test('RFI candidates pad only supported anchored RFI identifiers', () {
    expect(canonicalRfi('AZE-SUN-INT-QMS-RFI-CIV-901'), 'AZE-SUN-INT-QMS-RFI-CIV-0901');
    expect(canonicalFollowUpRfi('CIV-9')?.$2, 'CIV-0009');
    expect(canonicalFollowUpRfi('CIV-0009')?.$2, 'CIV-0009');
    expect(canonicalRfi('AZE-SUN-INT-QMS-RFI-CIV-09010'), isNull);
    expect(canonicalFollowUpRfi('ITP-CIV-901'), isNull);
    expect(canonicalFollowUpRfi('2026-09-01'), isNull);
    expect(parseRfiGroup('CIV-901/902'), isNotNull);
    expect(parseRfiGroup('ITP-CIV-901'), isNull);
    expect(canonicalFollowUpRfi('RFI-CIV-901')?.$1, 'AZE-SUN-INT-QMS-RFI-CIV-0901');
    expect(parseRfiGroup('RFI-CIV-901'), ['AZE-SUN-INT-QMS-RFI-CIV-0901']);
  });

  test('compound RFI parser expands historical forms without guessing', () {
    final parsed = parseRfiGroup('CIV-71-77/ 100/101/162/163')!;
    expect(parsed.length, 11);
    expect(parsed.first, 'AZE-SUN-INT-QMS-RFI-CIV-0071');
    expect(parsed.last, 'AZE-SUN-INT-QMS-RFI-CIV-0163');
    expect(formatRfiGroup(parseRfiGroup('MEC-0370/0565')!), 'MEC-0370/0565');
    expect(parseRfiGroup('CVI-0028'), isNull);
    expect(parseRfiGroup('MEC-02Â49'), isNull);
    expect(parseRfiGroup('MEC˗0249'), isNull);
    expect(parseRfiGroup('CIV-0100/MEC-0101'), isNull);
    expect(parseRfiGroup('CIV-0200-0100'), isNull);
  });

  test('approved status policy removes terminal failures and aggregates groups', () {
    expect(logState('Cancelled'), LogState.remove);
    expect(logState('Rejected'), LogState.remove);
    expect(aggregateStatus([LogState.pending]), 'Pending');
    expect(aggregateStatus([LogState.completed]), 'Completed');
    expect(aggregateStatus([LogState.completed, LogState.pending]), 'In Progress');
    expect(aggregateStatus([LogState.remove]), '');
  });

  test('Worley marker requires an exact line', () {
    expect(mappedFollowUpStatus('Signed by Worley'), 'Signed');
    expect(mappedFollowUpStatus('Note\nSigned by Worley\nOther'), 'Signed');
    expect(mappedFollowUpStatus('Not Signed by Worley'), 'Not Signed');
    expect(mappedFollowUpStatus('Signed by Worley team'), 'Not Signed');
  });

  test('projection budget exact boundary and one byte over', () {
    const limit = 32 * 1024 * 1024;
    final writer = FollowUpByteBudget(limit);
    final chunk = Uint8List(4096);
    for (var index = 0; index < limit ~/ chunk.length; index++) {
      writer.write(chunk);
    }
    expect(writer.count, limit);
    expect(() => writer.write([0]), throwsStateError);
    expect(writer.count, limit);
  });

  test('follow up authority rejects unknown fields and duplicate updates', () {
    final validMeta = _stepMeta([_approvedUpdate()]).toJson();
    final valid = Step(
      kind: StepKind.workbookUpdate,
      src: '/target.xlsx',
      dst: '/backup.xlsx',
      noteKey: 'note.rfi.followUp.workbookUpdate',
      meta: validMeta,
    );
    expect(() => decodeStepMeta(valid), returnsNormally);

    final unknown = Step(
      kind: valid.kind,
      src: valid.src,
      dst: valid.dst,
      noteKey: valid.noteKey,
      meta: {...validMeta, 'unknown': true},
    );
    expect(() => decodeStepMeta(unknown), throwsA(isA<BackendError>()));

    final update = _approvedUpdate();
    final duplicate = Step(
      kind: valid.kind,
      src: valid.src,
      dst: valid.dst,
      noteKey: valid.noteKey,
      meta: _stepMeta([update, update]).toJson(),
    );
    expect(() => decodeStepMeta(duplicate), throwsA(isA<BackendError>()));
  });

  test('plan from classification emits one exact workbook step for enabled rows', () {
    final update = _approvedUpdate();
    final row = FollowUpRow(
      rowId: update.rowId,
      sheet: 'Follow Up',
      row: 2,
      rfiNo: 'CIV-0001',
      sourceStatuses: const ['Signed'],
      currentStatus: '',
      proposedStatus: update.proposedStatus,
      proposedRfiNo: update.proposedRfi,
      disposition: FollowUpDisposition.ready,
      reasonKey: 'warn.rfi.followUp.reason.readySigned',
      selectable: true,
      enabled: true,
      target: FollowUpReviewTarget(
        worksheet: _descriptor(),
        row: 2,
        rfiColumn: 1,
        statusColumn: 2,
        rfiCell: update.target.rfiCell,
        statusCell: update.target.statusCell,
      ),
      sourceEvidence: const [],
      mappingDetails: null,
    );
    final layout = FollowUpLayout(
      worksheet: _descriptor(),
      headerRow: 1,
      rfiColumn: 1,
      statusColumn: 2,
    );
    final classification = <String, Object?>{
      'layout': layout.toJson(),
      'rows': [row.toJson()],
      'warnings': <Object?>[],
      'editor': editorMetadata([row], editorColumns()),
    };

    final plan = planFromClassification(
      rfiLogWorkbook: '/source.xlsx',
      followUpWorkbook: '/follow-up.xlsx',
      backupWorkbook: '/.docflow-s5-backup-test.xlsx',
      rfiLogFingerprint: _fingerprint(),
      followUpFingerprint: _fingerprint(),
      classification: classification,
    );

    expect(plan.opId, followUpOperationId);
    expect(plan.planId, isEmpty);
    expect(plan.steps.length, 1);
    expect(plan.steps[0].kind, StepKind.workbookUpdate);
    expect(plan.steps[0].src, '/follow-up.xlsx');
    expect(plan.steps[0].dst, '/.docflow-s5-backup-test.xlsx');
    expect(plan.steps[0].noteKey, 'note.rfi.followUp.workbookUpdate');
    expect(
      decodeStepMeta(plan.steps[0]).updates.map((item) => item.toJson()).toList(),
      [update.toJson()],
    );
    expect(plan.meta.containsKey('editor'), isTrue);
    expect(plan.meta.containsKey('rfiFollowUp'), isTrue);
  });

  test('source group analysis runs once for 500 duplicate targets', () {
    followUpSourceAnalysisCalls = 0;
    final sourceNames = [for (final columns in followUpStatusColumns) columns.sheet];
    final civStatusColumn = followUpStatusColumns[0].statusColumn;
    final sourceDiscovery = OrderedWorkbookDiscovery(
      worksheets: [
        for (var index = 0; index < sourceNames.length; index++)
          WorksheetDescriptor(
            workbookIndex: index + 1,
            sheetId: index + 1,
            relationshipId: 'rId${index + 1}',
            relationshipType: 'worksheet',
            partPath: 'xl/worksheets/sheet${index + 1}.xml',
            name: sourceNames[index],
            visibility: WorksheetVisibility.visible,
          ),
      ],
      safety: const WorkbookSafetyFacts(),
    );
    const rfi = 'AZE-SUN-INT-QMS-RFI-CIV-0006';
    final sourceWorkbook = WorkbookData(
      sheets: {
        for (final name in sourceNames) name: WorksheetData(),
        'CIV': WorksheetData(
          maxRow: 10,
          maxColumn: civStatusColumn,
          cells: {
            (10, 1): const CellData(value: rfi),
            (10, civStatusColumn): const CellData(value: 'Signed by Worley'),
          },
        ),
      },
    );

    final targetDiscovery = OrderedWorkbookDiscovery(
      worksheets: [
        WorksheetDescriptor(
          workbookIndex: 1,
          sheetId: 1,
          relationshipId: 'rId1',
          relationshipType: 'worksheet',
          partPath: 'xl/worksheets/sheet1.xml',
          name: 'Follow Up',
          visibility: WorksheetVisibility.visible,
        ),
      ],
      safety: const WorkbookSafetyFacts(),
    );
    final target = WorksheetData(
      maxRow: 501,
      maxColumn: 2,
      cells: {
        (1, 1): const CellData(value: 'RFI NO.'),
        (1, 2): const CellData(value: 'STATUS'),
        for (var row = 2; row <= 501; row++) (row, 1): const CellData(value: rfi),
        for (var row = 2; row <= 501; row++) (row, 2): const CellData(),
      },
    );
    final targetWorkbook = WorkbookData(sheets: {'Follow Up': target});

    classifyWorkbooksLegacy(
      sourceDiscovery,
      sourceWorkbook,
      targetDiscovery,
      targetWorkbook,
      targetHasSignature: false,
    );
    expect(followUpSourceAnalysisCalls, 1);
  });

  test('merge query budget fails at the first comparison over limit', () {
    final worksheet = WorksheetData(
      mergedRanges: const [
        CellRange(firstRow: 1, firstColumn: 1, lastRow: 1, lastColumn: 1),
      ],
    );
    final budget = FollowUpMergeBudget(1);
    expect(budget.contains(worksheet, 1, 1), isTrue);
    expect(
      () => budget.contains(worksheet, 1, 1),
      throwsBackendMessage('error.rfi.followUp.limitExceeded'),
    );
  });
}
