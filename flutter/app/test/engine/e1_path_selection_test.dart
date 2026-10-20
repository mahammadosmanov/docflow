// Port of the inline tests in src-tauri/src/path_selection.rs.
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/path_selection.dart';
import 'package:docflow/engine/paths.dart';

void main() {
  test('selected paths are opaque, exact-scoped and caller-bound', () {
    final temporary = Directory.systemTemp.createTempSync('e1_selection');
    addTearDown(() => temporary.deleteSync(recursive: true));
    final selected = File('${temporary.path}/selected.pdf')
      ..writeAsBytesSync([1]);
    final sibling = File('${temporary.path}/sibling.pdf')
      ..writeAsBytesSync([2]);
    const caller = CallerIdentity('owner', 'session');
    final store = PathSelectionStore(ttl: const Duration(seconds: 60));
    final reference = store.issue(
      caller,
      'files.batch_print',
      'files',
      selected.path,
      SelectionKind.file,
    );

    expect(reference.selectionId, isNot(selected.path));
    expect(
      store.resolve(
        reference,
        caller,
        'files.batch_print',
        'files',
        SelectionKind.file,
      ),
      canonicalizePath(selected.path),
    );
    expect(
      () => store.resolve(
        reference,
        caller,
        'rfi.log_add',
        'workbook',
        SelectionKind.file,
      ),
      throwsA(
        isA<BackendError>().having(
          (error) => error.code,
          'code',
          'path_grant_scope_mismatch',
        ),
      ),
    );
    expect(
      () => store.resolveScope(
        reference.selectionId,
        const CallerIdentity('other', 's'),
        SelectionScope.revealReviewItem,
      ),
      throwsA(
        isA<BackendError>().having(
          (error) => error.code,
          'code',
          'path_grant_scope_mismatch',
        ),
      ),
    );
    expect(
      canonicalizePath(selected.path),
      isNot(canonicalizePath(sibling.path)),
    );
  });

  test('review, reveal and undo consumers reject cross-operation and parameter capabilities', () {
    final temporary = Directory.systemTemp.createTempSync(
      'e1_selection_scopes',
    );
    addTearDown(() => temporary.deleteSync(recursive: true));
    final file = File('${temporary.path}/review.pdf')..writeAsBytesSync([3]);
    const caller = CallerIdentity('owner', 'session');
    final store = PathSelectionStore(ttl: const Duration(seconds: 60));
    final processDestination = store.issue(
      caller,
      'rfi.process_scans',
      'dest',
      temporary.path,
      SelectionKind.folder,
    );
    final reviewDestination = store.issue(
      caller,
      'rfi.review',
      'destination',
      temporary.path,
      SelectionKind.folder,
    );
    final reviewItem = store.issue(
      caller,
      'rfi.review',
      'item',
      file.path,
      SelectionKind.file,
    );
    final undoDestination = store.issue(
      caller,
      'history.undo',
      'destination',
      temporary.path,
      SelectionKind.folder,
    );

    final mismatches = <(String, SelectionScope)>[
      (processDestination.selectionId, SelectionScope.reviewDestination),
      (reviewItem.selectionId, SelectionScope.reviewDestination),
      (reviewDestination.selectionId, SelectionScope.reviewItem),
      (undoDestination.selectionId, SelectionScope.revealReviewItem),
      (reviewDestination.selectionId, SelectionScope.undoDestination),
    ];
    for (final (selectionId, scope) in mismatches) {
      expect(
        () => store.resolveScope(selectionId, caller, scope),
        throwsA(
          isA<BackendError>().having(
            (error) => error.code,
            'code',
            'path_grant_scope_mismatch',
          ),
        ),
      );
    }

    expect(
      Directory(
        store.resolveScope(
          reviewDestination.selectionId,
          caller,
          SelectionScope.reviewDestination,
        ),
      ).existsSync(),
      isTrue,
    );
    expect(
      store.resolveScope(
        reviewItem.selectionId,
        caller,
        SelectionScope.revealReviewItem,
      ),
      canonicalizePath(file.path),
    );
    expect(
      Directory(
        store.resolveScope(
          undoDestination.selectionId,
          caller,
          SelectionScope.undoDestination,
        ),
      ).existsSync(),
      isTrue,
    );
  });

  test('issue rejects a wrong-kind selection and invalid scope parameters', () {
    final temporary = Directory.systemTemp.createTempSync('e1_selection_kind');
    addTearDown(() => temporary.deleteSync(recursive: true));
    const caller = CallerIdentity('owner', 'session');
    final store = PathSelectionStore(ttl: const Duration(seconds: 60));
    expect(
      () => store.issue(caller, 'op', 'p', temporary.path, SelectionKind.file),
      throwsA(
        isA<BackendError>().having(
          (error) => error.code,
          'code',
          'path_invalid',
        ),
      ),
    );
    expect(
      () => store.issue(caller, '', 'p', temporary.path, SelectionKind.folder),
      throwsA(
        isA<BackendError>().having(
          (error) => error.code,
          'code',
          'backend_invalid_params',
        ),
      ),
    );
  });
}
