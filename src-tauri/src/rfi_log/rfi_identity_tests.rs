use super::*;

#[test]
fn rfi_log_identity_pads_only_rfi_anchored_serials() {
    assert_eq!(
        rfi_parts("AZE-SUN-INT-QMS-RFI-CIV-9"),
        Some(("CIV".to_string(), 9))
    );
    assert_eq!(
        rfi_parts("AZE-SUN-INT-QMS-RFI-CIV-901"),
        Some(("CIV".to_string(), 901))
    );
    assert!(rfi_parts("ITP-CIV-901").is_none());
    assert!(rfi_parts("2026-09-01").is_none());
    assert!(rfi_parts("AZE-SUN-INT-QMS-RFI-CIV-09010").is_none());
}
