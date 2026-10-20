// Port of src-tauri/tests/rfi_follow_up_current.rs and
// rfi_follow_up_proof.rs. Both Rust files are `#[ignore]` live-workbook checks
// that read operator-supplied files (env vars or /tmp proof paths). They need
// real XLSX/zip/XML patching and verification (E6 ooxml) and the authority
// preview path over real files, so they are carried as skipped entries here.
import 'package:flutter_test/flutter_test.dart';

void main() {
  // UNVERIFIED: requires DOCFLOW_CURRENT_RFI_LOG and DOCFLOW_CURRENT_FOLLOW_UP
  // operator workbooks and the E6 OOXML patch/verify pipeline on real files.
  test(
    'current workbooks produce a bounded guarded Follow Up plan',
    () {},
    skip: 'Requires operator-supplied current workbooks (env DOCFLOW_CURRENT_*) and E6 OOXML.',
  );

  // UNVERIFIED: requires /tmp/docflow_mac_proof RFI Log and Follow Up .xlsm
  // files and the E6 OOXML delta verification; not runnable in CI.
  test(
    'mac live proof runs Follow Up sync losslessly',
    () {},
    skip: 'Requires /tmp/docflow_mac_proof live workbooks and E6 OOXML verification.',
  );
}
