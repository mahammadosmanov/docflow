// Port of the inline tests in src-tauri/src/errors.rs.
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:docflow/engine/errors.dart';

void main() {
  test('backend error snapshots keep stable codes and protocol shape', () {
    expect(BackendError.protocol('invalid frame').toJson(), {
      'code': 'backend_protocol_error',
      'message': 'invalid frame',
    });
    expect(jsonDecode(jsonEncode(BackendError.planUnavailable().toJson())), {
      'code': 'plan_not_found_or_expired',
      'message': 'plan not found or expired; preview again',
    });
    expect(BackendError.operationInProgress().code, 'operation_in_progress');
  });
}
