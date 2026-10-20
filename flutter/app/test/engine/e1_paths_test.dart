// Port of the inline `provisional_tests` in src-tauri/src/paths.rs.
//
// Uses the real capability layer from lib/engine/cap_fs.dart (E2) through the
// default CapabilityRootOpener. The Windows-only dual-grant test
// (`#[cfg(windows)]`) is not ported here.
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:docflow/engine/errors.dart';
import 'package:docflow/engine/paths.dart';

PathGrantStore _store() => PathGrantStore(ttl: const Duration(seconds: 300));

Matcher _grantUnavailable() => isA<BackendError>().having(
  (error) => error.code,
  'code',
  'path_grant_not_found_or_expired',
);

void main() {
  late Directory fixture;

  setUp(() {
    fixture = Directory(
      Directory.systemTemp.createTempSync('e1_paths').resolveSymbolicLinksSync(),
    );
  });

  tearDown(() {
    fixture.deleteSync(recursive: true);
  });

  test('issue provisional revokes uncommitted grants and preserves committed grants', () {
    final first = Directory('${fixture.path}/first')..createSync();
    final second = Directory('${fixture.path}/second')..createSync();
    final store = _store();
    final scope = GrantScope.create('owner', 'session', 'request', 'op.test');

    final dropped = store.issueProvisional(scope, [
      (first.path, GrantAccess.read),
      (second.path, GrantAccess.readWrite),
    ]);
    final droppedId = dropped.grants[0].grantId;
    // Rust drops the provisional set here; Dart calls the explicit equivalent.
    dropped.revokeUnlessCommitted();
    expect(
      () => store.authorize(droppedId, scope, first.path, PathIntent.read),
      throwsA(_grantUnavailable()),
    );

    final provisional = store.issueProvisional(scope, [
      (first.path, GrantAccess.read),
    ]);
    final committedId = provisional.grants[0].grantId;
    provisional.commit();
    provisional.revokeUnlessCommitted();
    expect(
      store.authorize(committedId, scope, first.path, PathIntent.read),
      canonicalizePath(first.path),
    );
  });

  test(
    'authorize rejects another scope, a missing grant and lexical traversal',
    () {
      final root = Directory('${fixture.path}/root')..createSync();
      final store = _store();
      final scope = GrantScope.create('owner', 'session', 'request', 'op.test');
      final other = GrantScope.create(
        'owner',
        'session',
        'request',
        'op.other',
      );
      final grant = store.issue(scope, root.path, GrantAccess.read);

      expect(
        () => store.authorize(grant.grantId, other, root.path, PathIntent.read),
        throwsA(
          isA<BackendError>().having(
            (error) => error.code,
            'code',
            'path_grant_scope_mismatch',
          ),
        ),
      );
      expect(
        () => store.authorize('missing', scope, root.path, PathIntent.read),
        throwsA(_grantUnavailable()),
      );
      expect(
        () => store.authorize(
          grant.grantId,
          scope,
          '${root.path}/../escape',
          PathIntent.read,
        ),
        throwsA(
          isA<BackendError>().having(
            (error) => error.code,
            'code',
            'path_traversal',
          ),
        ),
      );
      expect(store.activeGrantCountForTesting, 1);
    },
  );

  test('read-only grants deny write intent', () {
    final root = Directory('${fixture.path}/readonly')..createSync();
    final store = _store();
    final scope = GrantScope.create('owner', 'session', 'request', 'op.test');
    final grant = store.issue(scope, root.path, GrantAccess.read);
    expect(
      () => store.authorize(
        grant.grantId,
        scope,
        '${root.path}/new.pdf',
        PathIntent.write,
      ),
      throwsA(
        isA<BackendError>().having(
          (error) => error.code,
          'code',
          'path_access_denied',
        ),
      ),
    );
  });

  test('revalidate for plan refuses candidates outside the granted root', () {
    final root = Directory('${fixture.path}/plan_root')..createSync();
    final outside = Directory('${fixture.path}/outside')..createSync();
    final store = _store();
    const caller = CallerIdentity('owner', 'session');
    final scope = GrantScope(
      caller: caller,
      requestId: 'request',
      operationId: 'op.plan',
    );
    final grant = store.issue(scope, root.path, GrantAccess.readWrite);

    expect(
      store.revalidateForPlan(
        [grant.grantId],
        caller,
        'op.plan',
        '${root.path}/inside.pdf',
        PathIntent.write,
      ),
      '${root.path}/inside.pdf',
    );
    expect(
      () => store.revalidateForPlan(
        [grant.grantId],
        caller,
        'op.plan',
        '${outside.path}/escape.pdf',
        PathIntent.write,
      ),
      throwsA(
        isA<BackendError>().having(
          (error) => error.code,
          'code',
          'path_escape',
        ),
      ),
    );
  });
}
