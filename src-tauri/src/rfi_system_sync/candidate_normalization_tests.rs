use super::*;

#[test]
fn sync_identity_accepts_only_anchored_supported_rfi_suffixes() {
    assert_eq!(
        exact_canonical_rfi("AZE-SUN-INT-QMS-RFI-CIV-901").as_deref(),
        Some("AZE-SUN-INT-QMS-RFI-CIV-0901")
    );
    assert_eq!(
        exact_canonical_rfi("CIV-9").as_deref(),
        Some("AZE-SUN-INT-QMS-RFI-CIV-0009")
    );
    assert!(exact_canonical_rfi("ITP-CIV-901").is_none());
    assert!(exact_canonical_rfi("2026-09-01").is_none());
    assert!(exact_canonical_rfi("AZE-SUN-INT-QMS-RFI-CIV-09010").is_none());
}
