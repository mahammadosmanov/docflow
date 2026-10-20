import 'dart:typed_data';

import 'package:crypto/crypto.dart' as crypto;

// The print types are defined once, in print_queue.dart. This library re-exports
// them so excel callers and PrintService share the same classes.
export '../print_queue.dart'
    show
        CleanupReview,
        FileSignature,
        PrintFailure,
        PrintFailureKind,
        PrintJob,
        PrintSettings,
        PrintSubmissionOutcome,
        RetryDisposition,
        fileSignatureFromFile;

/// Bytes helper retained for callers that need a digest of raw content.
Uint8List sha256Bytes(List<int> bytes) =>
    Uint8List.fromList(crypto.sha256.convert(bytes).bytes);
