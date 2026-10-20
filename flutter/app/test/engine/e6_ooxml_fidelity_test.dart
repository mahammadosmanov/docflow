// Port of src-tauri/tests/ooxml_fidelity.rs.
//
// UNVERIFIED: the Rust fixture hash assertion depends on zlib deflate output;
// the Dart fixture is checked for determinism instead of the Rust constant.
// UNVERIFIED: the Rust Unix pinned-handle test runs on Linux here via
// dart:io handle semantics; Windows and Office acceptance are not covered.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:archive/archive.dart';
import 'package:crypto/crypto.dart';
import 'package:docflow/engine/ooxml.dart';
import 'package:flutter_test/flutter_test.dart';

import 'e6_ooxml_fixtures.dart';

const String _strictMain = 'http://purl.oclc.org/ooxml/spreadsheetml/main';
const String _strictOfficeRel =
    'http://purl.oclc.org/ooxml/officeDocument/relationships';
const String _mainNs = mainNs;
const String _officeRel =
    'http://schemas.openxmlformats.org/officeDocument/2006/relationships';
const String _packageRel =
    'http://schemas.openxmlformats.org/package/2006/relationships';

Uint8List _bytes(String text) => Uint8List.fromList(utf8.encode(text));

/// Port of worksheet_facts_package_with_shared_strings.
Uint8List _worksheetFactsPackage(
  String namespace,
  List<(String, String?, String)> sheets, {
  bool workbookProtected = false,
  bool signed = false,
  String? sharedStrings,
}) {
  final officeNs = namespace == _strictMain ? _strictOfficeRel : _officeRel;
  final workbookSheets = StringBuffer();
  final workbookRelationships = StringBuffer();
  final members = <(String, List<int>)>[];
  for (var index = 0; index < sheets.length; index++) {
    final ordinal = index + 1;
    final (name, state, worksheet) = sheets[index];
    final stateAttr = state == null ? '' : ' state="$state"';
    workbookSheets.write(
      '<sheet name="$name" sheetId="$ordinal" r:id="rId$ordinal"$stateAttr/>',
    );
    workbookRelationships.write(
      '<Relationship Id="rId$ordinal" Type="$officeNs/worksheet" '
      'Target="worksheets/sheet$ordinal.xml"/>',
    );
    members.add(('xl/worksheets/sheet$ordinal.xml', utf8.encode(worksheet)));
  }
  if (sharedStrings != null) {
    workbookRelationships.write(
      '<Relationship Id="rIdSharedStrings" Type="$officeNs/sharedStrings" '
      'Target="sharedStrings.xml"/>',
    );
  }
  final signatureRelationship = signed
      ? '<Relationship Id="rIdSignature" Type="http://schemas.openxmlformats.org/package/2006/relationships/digital-signature/origin" Target="_xmlsignatures/origin.sigs"/>'
      : '';
  final packageRelationships =
      '<Relationships xmlns="$_packageRel"><Relationship Id="rIdWorkbook" '
      'Type="$officeNs/officeDocument" Target="xl/workbook.xml"/>'
      '$signatureRelationship</Relationships>';
  final protection = workbookProtected
      ? '<workbookProtection lockStructure="1"/>'
      : '';
  final workbook =
      '<workbook xmlns="$namespace" xmlns:r="$officeNs">$protection'
      '<sheets>$workbookSheets</sheets></workbook>';
  final relationships =
      '<Relationships xmlns="$_packageRel">$workbookRelationships</Relationships>';

  final head = <(String, List<int>)>[
    ('[Content_Types].xml', utf8.encode('content-types')),
    ('_rels/.rels', utf8.encode(packageRelationships)),
    ('xl/workbook.xml', utf8.encode(workbook)),
    ('xl/_rels/workbook.xml.rels', utf8.encode(relationships)),
  ];
  final tail = <(String, List<int>)>[
    if (signed) ('_xmlsignatures/origin.sigs', utf8.encode('signature-origin')),
    if (sharedStrings != null)
      ('xl/sharedStrings.xml', utf8.encode(sharedStrings)),
  ];
  return zipOf([...members.take(sheets.length), ...head, ...tail]);
}

Uint8List _member(Uint8List package, String name) => readMember(package, name);

/// Replaces one member, keeping every other member's raw bytes.
Uint8List _replaceMember(Uint8List package, String name, Uint8List bytes) =>
    OfficePackage.open(package).write({name: bytes});

/// Port of the Rust `package()` fixture: deflated members with a unix mode,
/// a stored vbaProject.bin, and an archive comment.
Uint8List _fixturePackage({required bool macros}) {
  final archive = Archive();
  void add(String name, List<int> bytes, {bool stored = false}) {
    final file = ArchiveFile(name, bytes.length, bytes)
      ..mode =
          0x1a0 // 0o640
      ..compress = !stored;
    archive.addFile(file);
  }

  add('[Content_Types].xml', _bytes(_contentTypes).toList());
  add('_rels/.rels', _bytes(_rootRelationships).toList());
  add('xl/workbook.xml', _bytes(_fixtureWorkbook).toList());
  add(
    'xl/_rels/workbook.xml.rels',
    _bytes(_fixtureWorkbookRelationships).toList(),
  );
  add('xl/worksheets/sheet1.xml', _bytes(_fixtureSheet1).toList());
  for (final (name, marker) in [
    ('xl/worksheets/sheet2.xml', 'MEC SHEET'),
    ('xl/worksheets/sheet3.xml', 'LOOKUP SHEET'),
    ('xl/worksheets/sheet4.xml', 'UNTOUCHED SHEET'),
  ]) {
    add(
      name,
      _bytes(
        '<worksheet xmlns="$_mainNs"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>$marker</t></is></c></row><row r="2"><c r="A2"><f>SUM(1,2)</f><v>3</v></c></row></sheetData></worksheet>',
      ).toList(),
    );
  }
  add(
    'xl/calcChain.xml',
    _bytes(
      '<calcChain xmlns="$_mainNs"><c r="A11" i="1"/><c r="D11"/><c r="A11" i="4"/></calcChain>',
    ).toList(),
  );
  for (final (name, bytes) in [
    ('xl/styles.xml', _bytes('styles-sentinel')),
    ('xl/theme/theme1.xml', _bytes('theme-sentinel')),
    ('xl/worksheets/_rels/sheet1.xml.rels', _bytes(_sheet1Relationships)),
    ('xl/drawings/drawing1.xml', _bytes('drawing-sentinel')),
    ('xl/comments1.xml', _bytes('comments-sentinel')),
    ('xl/externalLinks/externalLink1.xml', _bytes('external-link-sentinel')),
    ('docProps/core.xml', _bytes('metadata-sentinel')),
    ('customXml/item1.xml', _bytes('custom-xml-sentinel')),
  ]) {
    add(name, bytes.toList());
  }
  if (macros) {
    add(
      'xl/vbaProject.bin',
      latin1.encode('VBA\u0000PROJECTÿSENTINEL'),
      stored: true,
    );
  }
  final encoded = Uint8List.fromList(ZipEncoder().encode(archive)!);
  return _withArchiveComment(encoded, 'DocFlow fixture');
}

/// Sets the end-of-central-directory comment of a freshly encoded package.
Uint8List _withArchiveComment(Uint8List zip, String comment) {
  final eocd = finalEocdOffset(zip);
  final comments = utf8.encode(comment);
  final head = Uint8List.fromList(zip.sublist(0, eocd + 22));
  head[eocd + 20] = comments.length & 0xff;
  head[eocd + 21] = (comments.length >> 8) & 0xff;
  return Uint8List.fromList([...head, ...comments]);
}

const String _contentTypes =
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">\n'
    ' <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>\n'
    ' <Default Extension="xml" ContentType="application/xml"/>\n'
    ' <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>\n'
    '</Types>';

const String _rootRelationships =
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>';

const String _fixtureWorkbook =
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<workbook xmlns="$_mainNs" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="CIV" sheetId="1" r:id="rId1"/><sheet name="MEC" sheetId="2" r:id="rId2"/><sheet name="Sheet1" sheetId="3" r:id="rId3"/><sheet name="Untouched" sheetId="4" r:id="rId4"/></sheets><definedNames><definedName name="Print_Area" localSheetId="0">CIV!\$A\$1:\$Q\$30</definedName><definedName name="ExternalName">\'[linked.xlsx]Data\'!\$A\$1</definedName></definedNames><calcPr calcId="191029" calcMode="manual"/></workbook>';

const String _fixtureWorkbookRelationships =
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet3.xml"/><Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet4.xml"/></Relationships>';

const String _sheet1Relationships =
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdDraw" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>';

const String _fixtureSheet1 =
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<worksheet xmlns="$_mainNs" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:x14ac="http://schemas.microsoft.com/office/spreadsheetml/2009/9/ac" xmlns:xr2="http://schemas.microsoft.com/office/spreadsheetml/2015/revision2" mc:Ignorable="x14ac xr2"><dimension ref="A1:Q30"/><cols><col min="1" max="1" width="25.25" customWidth="1"/></cols><sheetData><row r="9"><c r="A9" t="inlineStr"><is><t>RFI No.</t></is></c><c r="D9" t="inlineStr"><is><t>DESCRIPTION</t></is></c></row><row r="10"><c r="A10" s="4" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0007</t></is></c></row><row r="11"><c r="A11" s="4"><f>1+1</f><v>2</v></c><c r="D11" s="9" t="str"><f t="shared" si="7">C11*2</f><v/></c><c r="J11" s="12" t="inlineStr"><is><t>KEEP EXPIRY</t></is></c></row><row r="30"><c r="Q30"/></row></sheetData><dataValidations count="2"><dataValidation type="custom" sqref="A10:A30"><formula1>COUNTIF(\$A\$10:\$A\$30,A10)&lt;=1</formula1></dataValidation><dataValidation type="list" sqref="C10:C30"><formula1>INDIRECT(B10)</formula1></dataValidation></dataValidations><drawing r:id="rIdDraw"/></worksheet>';

/// Port of the `EntryEvidence` capture: names, archive comment, and
/// per-member evidence keyed by name.
({List<String> names, Uint8List comment, Map<String, ZipEntryEvidence> entries})
_evidence(Uint8List package) {
  final package0 = OfficePackage.open(package);
  final evidence = package0.evidence();
  return (
    names: [for (final entry in evidence) entry.name],
    comment: package0.comment,
    entries: {for (final entry in evidence) entry.name: entry},
  );
}

Map<String, Map<(int, int), GuardedCellUpdate>> _followUpUpdate({
  String replacement = 'Signed',
}) => {
  'Follow Up': {
    (2, 2): GuardedCellUpdate(
      expected: CellData(style: '7'),
      replacement: CellUpdateText(replacement),
      createIfAbsent: false,
    ),
  },
};

void _expectInvalid(void Function() action) {
  expect(
    action,
    throwsA(
      isA<OoxmlError>().having(
        (error) => error.kind,
        'kind',
        OoxmlErrorKind.invalid,
      ),
    ),
  );
}

void main() {
  test('guarded text patch can materialize only a missing cell in an existing row', () {
    final sheet =
        '<worksheet xmlns="$_mainNs"><sheetData><row r="2"><c r="A2" t="inlineStr"><is><t>RFI</t></is></c></row></sheetData></worksheet>';
    final source = _worksheetFactsPackage(_mainNs, [('CIV', null, sheet)]);
    final updates = {
      'CIV': {
        (2, 17): GuardedCellUpdate(
          expected: const CellData(),
          replacement: const CellUpdateText(
            'RFI Status Sync: CIV-1222 Completed',
          ),
          createIfAbsent: true,
        ),
      },
    };

    final destination = patchWorkbookSheetsGuardedText(source, updates);
    verifyGuardedWorkbookDeltaText(source, destination, updates);
    final workbook = readWorkbook(destination);

    expect(
      workbook.sheet('CIV')!.value(2, 17),
      'RFI Status Sync: CIV-1222 Completed',
    );
  });

  test('guarded text patch rejects creating a missing row', () {
    final sheet =
        '<worksheet xmlns="$_mainNs"><sheetData><row r="2"><c r="A2" t="inlineStr"><is><t>RFI</t></is></c></row></sheetData></worksheet>';
    final source = _worksheetFactsPackage(_mainNs, [('CIV', null, sheet)]);
    final updates = {
      'CIV': {
        (3, 17): GuardedCellUpdate(
          expected: const CellData(),
          replacement: const CellUpdateText(
            'RFI Status Sync: CIV-1222 Completed',
          ),
          createIfAbsent: true,
        ),
      },
    };

    expect(
      () => patchWorkbookSheetsGuardedText(source, updates),
      throwsA(isA<OoxmlError>()),
    );
  });

  test('guarded follow-up patch changes only authorized value events', () {
    for (final namespace in [_mainNs, _strictMain]) {
      final worksheet =
          '<worksheet xmlns="$namespace" xmlns:x="urn:sentinel"><sheetData><row r="2" spans="1:3" customFormat="1"><c r="A2" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0006</t></is></c><c r="B2" s="7" x:keep="yes" x:t="sentinel"><extLst><ext uri="urn:keep"><x:payload>unchanged</x:payload></ext></extLst></c><c r="C2" t="inlineStr"><is><t>do not change</t></is></c></row></sheetData><dataValidations count="1"><dataValidation type="list" sqref="B2"><formula1>Statuses</formula1></dataValidation></dataValidations><hyperlinks><hyperlink ref="C2" location="Sheet1!A1"/></hyperlinks><extLst><ext uri="urn:sheet"><x:payload>sheet sentinel</x:payload></ext></extLst></worksheet>';
      final source = _worksheetFactsPackage(namespace, [
        ('Follow Up', null, worksheet),
      ]);
      final beforeWorkbook = _member(source, 'xl/workbook.xml');
      final beforeRelationships = _member(source, 'xl/_rels/workbook.xml.rels');
      final updates = _followUpUpdate();

      final destination = patchWorkbookSheetsGuarded(source, updates);
      verifyGuardedWorkbookDelta(source, destination, updates);

      expect(_member(destination, 'xl/workbook.xml'), beforeWorkbook);
      expect(
        _member(destination, 'xl/_rels/workbook.xml.rels'),
        beforeRelationships,
      );
      final patched = utf8.decode(
        _member(destination, 'xl/worksheets/sheet1.xml'),
      );
      expect(
        patched,
        contains(
          '<c r="B2" s="7" x:keep="yes" x:t="sentinel" t="inlineStr"><is><t>Signed</t></is><extLst>',
        ),
      );
      expect(patched, contains('do not change'));
      expect(patched, contains('sheet sentinel'));
    }
  });

  test('guarded follow-up verifier compares expanded events, not attribute quote bytes', () {
    final worksheet =
        '<worksheet xmlns="$_mainNs"><sheetData><row r="2"><c r="A2" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0006</t></is></c><c r="B2" s="7"/></row></sheetData></worksheet>';
    final source = _worksheetFactsPackage(_mainNs, [
      ('Follow Up', null, worksheet),
    ]);
    final updates = _followUpUpdate();
    final destination = patchWorkbookSheetsGuarded(source, updates);
    final sheetBytes = _member(destination, 'xl/worksheets/sheet1.xml');
    final lexical = utf8.encode(
      utf8.decode(sheetBytes).replaceFirst('<row r="2">', "<row r='2'>"),
    );
    final equivalent = _replaceMember(
      destination,
      'xl/worksheets/sheet1.xml',
      Uint8List.fromList(lexical),
    );

    verifyGuardedWorkbookDelta(source, equivalent, updates);
  });

  test('guarded follow-up verifier rejects unapproved event and package deltas', () {
    final worksheet =
        '<worksheet xmlns="$_mainNs" xmlns:x="urn:sentinel"><sheetData><row r="2" spans="1:3"><c r="A2" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0006</t></is></c><c r="B2" s="7"/><c r="C2" t="inlineStr"><is><t>keep</t></is></c></row></sheetData><dataValidations count="1"><dataValidation type="list" sqref="B2"><formula1>Statuses</formula1></dataValidation></dataValidations><extLst><ext uri="urn:sheet"><x:payload>sentinel</x:payload></ext></extLst></worksheet>';
    final source = _worksheetFactsPackage(_mainNs, [
      ('Follow Up', null, worksheet),
    ]);
    final updates = _followUpUpdate();
    final destination = patchWorkbookSheetsGuarded(source, updates);
    final sheet = utf8.decode(_member(destination, 'xl/worksheets/sheet1.xml'));
    final workbook = utf8.decode(_member(destination, 'xl/workbook.xml'));
    final tampered = <(String, String)>[
      (
        'xl/worksheets/sheet1.xml',
        sheet.replaceFirst('spans="1:3"', 'spans="1:4"'),
      ),
      ('xl/worksheets/sheet1.xml', sheet.replaceFirst('>keep<', '>changed<')),
      ('xl/worksheets/sheet1.xml', sheet.replaceFirst('>Statuses<', '>Other<')),
      (
        'xl/worksheets/sheet1.xml',
        sheet.replaceFirst('>sentinel<', '>changed<'),
      ),
      (
        'xl/workbook.xml',
        workbook.replaceFirst('<workbook ', '<workbook calcId="1" '),
      ),
    ];
    for (final (name, text) in tampered) {
      final candidate = _replaceMember(
        destination,
        name,
        Uint8List.fromList(utf8.encode(text)),
      );
      _expectInvalid(
        () => verifyGuardedWorkbookDelta(source, candidate, updates),
      );
    }
  });

  test('guarded follow-up rejects invalid update authority before output', () {
    final worksheet =
        '<worksheet xmlns="$_mainNs"><sheetData><row r="2"><c r="A2" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0006</t></is></c><c r="B2" s="7"/></row></sheetData></worksheet>';
    final source = _worksheetFactsPackage(_mainNs, [
      ('Follow Up', null, worksheet),
    ]);
    const approvedExpected = CellData(style: '7');
    const approvedReplacement = CellUpdateText('Signed');
    final invalidSets = <Map<String, Map<(int, int), GuardedCellUpdate>>>[
      {},
      {
        'Follow Up': {
          (2, 2): const GuardedCellUpdate(
            expected: CellData(style: '8'),
            replacement: approvedReplacement,
            createIfAbsent: false,
          ),
        },
      },
      {
        'Follow Up': {
          (2, 2): const GuardedCellUpdate(
            expected: approvedExpected,
            replacement: CellUpdateText('Rejected'),
            createIfAbsent: false,
          ),
        },
      },
      {
        'Follow Up': {
          (3, 2): const GuardedCellUpdate(
            expected: approvedExpected,
            replacement: approvedReplacement,
            createIfAbsent: false,
          ),
        },
      },
    ];
    for (final updates in invalidSets) {
      expect(
        () => patchWorkbookSheetsGuarded(source, updates),
        throwsA(isA<OoxmlError>()),
      );
    }
  });

  test('guarded writer stays on pinned handles after visible parent swap', () {
    final fixture = Directory.systemTemp.createTempSync('e6_pinned_');
    addTearDown(() => fixture.deleteSync(recursive: true));
    final sourcePath = '${fixture.path}/Follow Up.xlsx';
    final visibleParent = Directory('${fixture.path}/.docflow-backups')
      ..createSync();
    final pinnedParent = '${fixture.path}/pinned-backups';
    final replacementParent = Directory('${fixture.path}/replacement-backups')
      ..createSync();
    final worksheet =
        '<worksheet xmlns="$_mainNs"><sheetData><row r="2"><c r="A2" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0006</t></is></c><c r="B2" s="7"/></row></sheetData></worksheet>';
    final package = _worksheetFactsPackage(_mainNs, [
      ('Follow Up', null, worksheet),
    ]);
    File(sourcePath).writeAsBytesSync(package);
    final outputPath = '${visibleParent.path}/private-output.xlsx';
    final handle = createNewOutputFile(outputPath);
    addTearDown(handle.closeSync);
    visibleParent.renameSync(pinnedParent);
    Link('${fixture.path}/.docflow-backups').createSync(replacementParent.path);
    final updates = _followUpUpdate();

    patchWorkbookSheetsGuardedToHandle(sourcePath, handle, updates);
    final source = File(sourcePath).readAsBytesSync();
    final written = readOutputHandle(handle);
    verifyGuardedWorkbookDelta(source, written, updates);

    expect(File('$pinnedParent/private-output.xlsx').existsSync(), isTrue);
    expect(
      File('${replacementParent.path}/private-output.xlsx').existsSync(),
      isFalse,
    );
  });

  test('worksheet facts accept only canonical strict or transitional hierarchy', () {
    for (final namespace in [_mainNs, _strictMain]) {
      final worksheet =
          '<worksheet xmlns="$namespace"><sheetData><row r="2"><c r="A2" t="inlineStr"><is><t>canonical</t></is></c></row></sheetData></worksheet>';
      final source = _worksheetFactsPackage(namespace, [
        ('Facts', null, worksheet),
      ]);
      expect(readWorkbook(source).sheet('Facts')!.value(2, 1), 'canonical');
    }

    final malformed = [
      '<worksheet xmlns="$_mainNs"><cols/></worksheet>',
      '<worksheet xmlns="$_mainNs"><sheetData/><sheetData><row r="2"><c r="A2"/></row></sheetData></worksheet>',
      '<worksheet xmlns="$_mainNs"><sheetData><c r="A2"/></sheetData></worksheet>',
      '<worksheet xmlns="$_mainNs"><sheetData><row r="3"><c r="A2"/></row></sheetData></worksheet>',
    ];
    for (final worksheet in malformed) {
      final source = _worksheetFactsPackage(_mainNs, [
        ('Facts', null, worksheet),
      ]);
      expect(
        () => readWorkbook(source),
        throwsA(isA<OoxmlError>()),
        reason: worksheet,
      );
    }
  });

  test('worksheet facts reject duplicate coordinates before indexing', () {
    final worksheet =
        '<worksheet xmlns="$_mainNs"><sheetData><row r="2"><c r="A2" t="inlineStr"><is><t>first</t></is></c><c r="A2" t="inlineStr"><is><t>second</t></is></c></row></sheetData></worksheet>';
    final source = _worksheetFactsPackage(_mainNs, [
      ('Facts', null, worksheet),
    ]);
    expect(() => readWorkbook(source), throwsA(isA<OoxmlError>()));
  });

  test('worksheet facts reject foreign namespace cell shadowing', () {
    final worksheet =
        '<worksheet xmlns="$_mainNs" xmlns:foreign="urn:foreign"><sheetData><row r="2"><foreign:c r="A2" t="inlineStr"><foreign:is><foreign:t>shadow</foreign:t></foreign:is></foreign:c></row></sheetData></worksheet>';
    final source = _worksheetFactsPackage(_mainNs, [
      ('Facts', null, worksheet),
    ]);
    expect(() => readWorkbook(source), throwsA(isA<OoxmlError>()));
  });

  test('worksheet facts reject foreign inline wrapper shadowing', () {
    final worksheet =
        '<worksheet xmlns="$_mainNs" xmlns:evil="urn:evil"><sheetData><row r="2"><c r="A2" t="inlineStr"><is><evil:r><t>shadow</t></evil:r></is></c></row></sheetData></worksheet>';
    final source = _worksheetFactsPackage(_mainNs, [
      ('Facts', null, worksheet),
    ]);
    expect(() => readWorkbook(source), throwsA(isA<OoxmlError>()));
  });

  test('worksheet facts reject formula outside canonical cell child chain', () {
    final worksheet =
        '<worksheet xmlns="$_mainNs" xmlns:evil="urn:evil"><sheetData><row r="2"><c r="A2"><evil:wrapper><f>LOOKUP()</f></evil:wrapper><v>Completed</v></c></row></sheetData></worksheet>';
    final source = _worksheetFactsPackage(_mainNs, [
      ('Facts', null, worksheet),
    ]);
    expect(() => readWorkbook(source), throwsA(isA<OoxmlError>()));
  });

  test('worksheet facts reject duplicate physical row identities', () {
    final worksheet =
        '<worksheet xmlns="$_mainNs"><sheetData><row r="2"><c r="A2"/></row><row r="2"><c r="B2"/></row></sheetData></worksheet>';
    final source = _worksheetFactsPackage(_mainNs, [
      ('Facts', null, worksheet),
    ]);
    expect(() => readWorkbook(source), throwsA(isA<OoxmlError>()));
  });

  test('worksheet facts reject noncanonical shared string namespace or hierarchy', () {
    final worksheet =
        '<worksheet xmlns="$_mainNs"><sheetData><row r="2"><c r="A2" t="s"><v>0</v></c></row></sheetData></worksheet>';
    final sharedStrings = [
      '<sst xmlns="urn:evil"><si><t>shadow</t></si></sst>',
      '<sst xmlns="$_mainNs" xmlns:evil="urn:evil"><evil:si><t>shadow</t></evil:si></sst>',
      '<sst xmlns="$_mainNs" xmlns:evil="urn:evil"><si><evil:wrapper><t>shadow</t></evil:wrapper></si></sst>',
    ];
    for (final shared in sharedStrings) {
      final source = _worksheetFactsPackage(_mainNs, [
        ('Facts', null, worksheet),
      ], sharedStrings: shared);
      expect(
        () => readWorkbook(source),
        throwsA(isA<OoxmlError>()),
        reason: shared,
      );
    }
  });

  test('worksheet facts capture formula, merge, protection, signature and visibility', () {
    final hidden =
        '<worksheet xmlns="$_mainNs"><sheetProtection sheet="1"/><sheetData><row r="2"><c r="A2" t="inlineStr"><is><t>merged</t></is></c></row><row r="3"><c r="C3"><f>1+1</f><v>2</v></c></row></sheetData><mergeCells count="1"><mergeCell ref="A2:B2"/></mergeCells></worksheet>';
    final veryHidden =
        '<worksheet xmlns="$_mainNs"><sheetData><row r="1"><c r="A1"/></row></sheetData></worksheet>';
    final source = _worksheetFactsPackage(
      _mainNs,
      [('Hidden', 'hidden', hidden), ('VeryHidden', 'veryHidden', veryHidden)],
      workbookProtected: true,
      signed: true,
    );

    final discovery = orderedWorkbookDiscovery(source);
    expect(discovery.safety.hasWorkbookProtection, isTrue);
    expect(
      [for (final sheet in discovery.worksheets) sheet.visibility],
      [WorksheetVisibility.hidden, WorksheetVisibility.veryHidden],
    );
    expect(packageHasSignature(source), isTrue);
    final workbook = readWorkbook(source);
    final sheet = workbook.sheet('Hidden')!;
    expect(sheet.hasSheetProtection, isTrue);
    expect(sheet.cell(3, 3)!.formula, '1+1');
    final merged = sheet.mergedRangeContaining(2, 2)!;
    expect(
      (merged.firstRow, merged.firstColumn, merged.lastRow, merged.lastColumn),
      (2, 1, 2, 2),
    );
  });

  test('targeted patch preserves all untouched members and zip properties', () {
    final source = _fixturePackage(macros: true);
    // UNVERIFIED: the Rust fixture hash (2c3fb5...) depends on zlib output, so
    // the Dart fixture is checked for determinism instead.
    expect(
      sha256.convert(source).toString(),
      sha256.convert(_fixturePackage(macros: true)).toString(),
    );
    final before = _evidence(source);

    final updates = {
      'CIV': <(int, int), CellUpdate>{
        (11, 1): const CellUpdateText('AZE-SUN-INT-QMS-RFI-CIV-0008'),
        (11, 4): const CellUpdateFormula(
          formula: 'C11*2',
          cached: 'calculated',
        ),
        (11, 8): const CellUpdateNumber('45500'),
        (11, 6): const CellUpdateText(' padded tag '),
      },
    };
    final destination = patchWorkbookSheets(
      source,
      updates,
      allowConsecutiveDuplicatesColumn: 1,
    );

    final after = _evidence(destination);
    expect(after.names, before.names);
    expect(after.comment, before.comment);
    const changed = {'xl/worksheets/sheet1.xml', 'xl/calcChain.xml'};
    for (final name in before.names) {
      if (!changed.contains(name)) {
        expect(after.entries[name], before.entries[name], reason: name);
      }
    }
    for (final name in changed) {
      expect(
        after.entries[name]!.method,
        before.entries[name]!.method,
        reason: name,
      );
      expect(
        after.entries[name]!.time,
        before.entries[name]!.time,
        reason: name,
      );
      expect(
        after.entries[name]!.date,
        before.entries[name]!.date,
        reason: name,
      );
      expect(
        after.entries[name]!.externalAttributes,
        before.entries[name]!.externalAttributes,
        reason: name,
      );
      expect(
        after.entries[name]!.comment,
        before.entries[name]!.comment,
        reason: name,
      );
    }
    expect(
      after.entries['xl/vbaProject.bin']!.content,
      latin1.encode('VBA\u0000PROJECTÿSENTINEL'),
    );

    final sheet = utf8.decode(
      after.entries['xl/worksheets/sheet1.xml']!.content,
    );
    expect(sheet, contains('AZE-SUN-INT-QMS-RFI-CIV-0008'));
    expect(sheet, contains('<f t="shared" si="7">C11*2</f>'));
    expect(sheet, contains('<v>calculated</v>'));
    expect(sheet, contains('s="4"'));
    expect(sheet, contains('s="9"'));
    expect(sheet, contains('xml:space="preserve"> padded tag </t>'));
    expect(sheet, contains('KEEP EXPIRY'));
    expect(sheet, contains('xmlns:xr2='));
    expect(sheet, contains('mc:Ignorable="x14ac xr2"'));
    expect(
      sheet,
      contains('OR(A10=&quot;&quot;,COUNTIF(\$A\$10:A10,A10)=1,A10=A9)'),
    );
    expect(sheet, contains('INDIRECT(B10)'));
    final chain = utf8.decode(after.entries['xl/calcChain.xml']!.content);
    expect(chain, isNot(contains('<c r="A11" i="1"')));
    expect(chain, contains('<c r="D11" i="1"'));
    expect(chain, contains('<c r="A11" i="4"'));
  });

  test(
    'patch rejects malformed input and existing destination without clobber',
    () {
      final fixture = Directory.systemTemp.createTempSync('e6_clobber_');
      addTearDown(() => fixture.deleteSync(recursive: true));
      final malformed = '${fixture.path}/malformed.xlsx';
      final output = '${fixture.path}/output.xlsx';
      File(malformed).writeAsBytesSync(utf8.encode('not a zip'));
      final updates = {
        'CIV': <(int, int), CellUpdate>{(1, 1): const CellUpdateText('value')},
      };

      expect(
        () => patchWorkbookSheetsFile(malformed, output, updates),
        throwsA(isA<OoxmlError>()),
      );
      expect(File(output).existsSync(), isFalse);

      final source = '${fixture.path}/source.xlsx';
      File(source).writeAsBytesSync(_fixturePackage(macros: false));
      File(output).writeAsBytesSync(utf8.encode('KEEP'));
      expect(
        () => patchWorkbookSheetsFile(source, output, updates),
        throwsA(isA<FileSystemException>()),
      );
      expect(File(output).readAsBytesSync(), utf8.encode('KEEP'));
    },
  );
}
