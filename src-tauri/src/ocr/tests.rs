use super::*;

#[test]
fn unconfirmed_page_source_distinguishes_empty_pages_from_ambiguous_codes() {
    let empty = [(150, find(""))];
    assert!(
        unconfirmed_page_source(&empty, Some(&BTreeSet::new())).starts_with("ocr:no-candidate:")
    );

    let conflicting = [
        (150, find("AZE-SUN-INT-QMS-RFI-CIV-0064")),
        (200, find("AZE-SUN-INT-QMS-RFI-MEC-0064")),
    ];
    assert!(
        unconfirmed_page_source(&conflicting, None).starts_with("ocr:ambiguous-or-unconfirmed:")
    );

    let searchable_code = BTreeSet::from(["AZE-SUN-INT-QMS-RFI-CIV-0064".to_string()]);
    assert!(
        unconfirmed_page_source(&empty, Some(&searchable_code))
            .starts_with("ocr:ambiguous-or-unconfirmed:")
    );
}

#[test]
fn rfi_consensus_rejects_one_conflicting_strict_digit_read() {
    let observations = [
        (300, find("AZE-SUN-INT-QMS-RFI-CIV-4286")),
        (250, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
        (200, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
        (350, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
    ];

    assert!(select_rfi_consensus(&observations, None).is_none());
}

#[test]
fn rfi_consensus_requires_three_distinct_render_resolutions() {
    let observations = [
        (300, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
        (300, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
        (250, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
    ];

    assert!(select_rfi_consensus(&observations, None).is_none());
}

#[test]
fn rfi_consensus_requires_two_exact_reads_not_only_repairs() {
    let observations = [
        (300, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
        (250, find("AZE-SUN-INT-QMS-RFI-CIV-I286")),
        (200, find("AZE-SUN-INT-QMS-RFI-CIV-I286")),
    ];

    assert!(select_rfi_consensus(&observations, None).is_none());
}

#[test]
fn rfi_consensus_accepts_two_exact_fallback_reads_after_consistent_repairs() {
    let observations = [
        (300, find("AZE-SUN-INT-QMS-RFI-CIV-O648")),
        (250, find("AZE-SUN-INT-QMS-RFI-CIV-O648")),
        (200, find("AZE-SUN-INT-QMS-RFI-CIV-O648")),
        (250, find("AZE-SUN-INT-QMS-RFI-CIV-0648")),
        (300, find("AZE-SUN-INT-QMS-RFI-CIV-0648")),
    ];

    let (matched, votes, dpi_count) =
        select_rfi_consensus(&observations, None).expect("exact fallback corroborates repairs");

    assert_eq!(matched.code, "AZE-SUN-INT-QMS-RFI-CIV-0648");
    assert_eq!((votes, dpi_count), (2, 3));
}

#[test]
fn rfi_consensus_accepts_one_exact_code_across_three_resolutions() {
    let observations = [
        (300, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
        (250, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
        (200, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
    ];

    let (matched, votes, dpi_count) = select_rfi_consensus(&observations, None).unwrap();

    assert_eq!(matched.code, "AZE-SUN-INT-QMS-RFI-CIV-1286");
    assert_eq!(matched.confidence, 0.95);
    assert_eq!((votes, dpi_count), (3, 3));
}

#[test]
fn short_and_zero_padded_reads_are_one_identity_but_distinct_codes_conflict() {
    let equivalent = [
        (300, find("AZE-SUN-INT-QMS-RFI-CIV-9")),
        (250, find("AZE-SUN-INT-QMS-RFI-CIV-0009")),
        (200, find("AZE-SUN-INT-QMS-RFI-CIV-0009")),
    ];
    assert_eq!(
        select_rfi_consensus(&equivalent, None).unwrap().0.code,
        "AZE-SUN-INT-QMS-RFI-CIV-0009"
    );

    let conflict = [
        (300, find("AZE-SUN-INT-QMS-RFI-CIV-901")),
        (250, find("AZE-SUN-INT-QMS-RFI-CIV-0902")),
        (200, find("AZE-SUN-INT-QMS-RFI-CIV-0902")),
    ];
    assert!(select_rfi_consensus(&conflict, None).is_none());
}

#[test]
fn malformed_alphanumeric_suffixes_cannot_become_ocr_consensus_identity() {
    for suffix in ["901A", "901А", "09010"] {
        let text = format!("AZE-SUN-INT-QMS-RFI-CIV-{suffix}");
        let matched = rfi_matches(&text).remove(0);
        let observations = [300, 250, 200].map(|dpi| (dpi, matched.clone()));
        assert!(
            observations.iter().all(|(_, matched)| !matched.ok()),
            "{suffix} produced an OCR identity"
        );
        assert!(select_rfi_consensus(&observations, None).is_none());
    }
}

#[test]
fn rfi_consensus_rejects_a_conflicting_searchable_text_layer() {
    let observations = [
        (300, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
        (250, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
        (200, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
    ];
    let stale_text = BTreeSet::from(["AZE-SUN-INT-QMS-RFI-CIV-4286".to_string()]);

    assert!(select_rfi_consensus(&observations, Some(&stale_text)).is_none());
}

#[test]
fn rfi_consensus_rejects_multiple_searchable_text_layer_codes() {
    let observations = [
        (300, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
        (250, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
        (200, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
    ];
    let ambiguous_text = BTreeSet::from([
        "AZE-SUN-INT-QMS-RFI-CIV-1286".to_string(),
        "AZE-SUN-INT-QMS-RFI-CIV-4286".to_string(),
    ]);

    assert!(select_rfi_consensus(&observations, Some(&ambiguous_text)).is_none());
}

#[test]
fn ambiguous_ocr_diagnostics_preserve_text_layer_evidence() {
    assert_eq!(
        text_layer_observation_summary(Some(&BTreeSet::from([
            "AZE-SUN-INT-QMS-RFI-CIV-1286".to_string(),
            "AZE-SUN-INT-QMS-RFI-CIV-4286".to_string(),
        ]))),
        "text-codes=AZE-SUN-INT-QMS-RFI-CIV-1286,AZE-SUN-INT-QMS-RFI-CIV-4286"
    );
    assert_eq!(text_layer_observation_summary(None), "no-text-layer");
}

#[test]
fn one_ocr_pass_with_multiple_exact_codes_is_ambiguous() {
    let matches = rfi_matches("AZE-SUN-INT-QMS-RFI-CIV-1286 AZE-SUN-INT-QMS-RFI-CIV-4286");

    assert_eq!(matches.len(), 2);
    assert!(
        matches
            .iter()
            .all(|matched| matched.method == RfiMethod::Strict)
    );
}

#[test]
fn production_discovery_ignores_environment_tesseract_overrides() {
    static ENVIRONMENT: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = ENVIRONMENT.lock().unwrap();
    let fixture = tempfile::tempdir().unwrap();
    let override_binary = fixture.path().join(if cfg!(windows) {
        "attacker-tesseract.exe"
    } else {
        "attacker-tesseract"
    });
    fs::write(&override_binary, b"not a bundled executable").unwrap();
    let previous = std::env::var_os("DOCFLOW_TESSERACT");
    // SAFETY: this focused test serializes its environment mutation and restores it below.
    unsafe { std::env::set_var("DOCFLOW_TESSERACT", &override_binary) };

    let discovered = NativeOcr::discover();

    match previous {
        Some(value) => {
            // SAFETY: the serialized test restores the prior process value.
            unsafe { std::env::set_var("DOCFLOW_TESSERACT", value) };
        }
        None => {
            // SAFETY: the serialized test restores the prior absence.
            unsafe { std::env::remove_var("DOCFLOW_TESSERACT") };
        }
    }
    assert_ne!(
        discovered.tesseract.as_deref(),
        Some(override_binary.as_path())
    );
}

#[test]
fn adaptive_preprocessing_crops_the_upper_band_and_emits_only_binary_pixels() {
    let image = GrayImage {
        width: 5,
        height: 10,
        pixels: (0_u8..50).map(|value| value.saturating_mul(5)).collect(),
    };

    let thresholded = adaptive_threshold(&image, 0.35, 3, 10).unwrap();

    assert_eq!((thresholded.width, thresholded.height), (5, 3));
    assert_eq!(thresholded.pixels.len(), 15);
    assert!(
        thresholded
            .pixels
            .iter()
            .all(|value| matches!(value, 0 | 255))
    );
}

#[test]
fn thumbnail_resize_and_base64_are_bounded_and_canonical() {
    let image = GrayImage {
        width: 4,
        height: 2,
        pixels: vec![0, 1, 2, 3, 4, 5, 6, 7],
    };
    let resized = resize_gray_to_fit(&image, 2, 2).unwrap();
    assert_eq!((resized.width, resized.height), (2, 1));
    assert_eq!(resized.pixels, vec![0, 2]);
    assert_eq!(base64_encode(b""), "");
    assert_eq!(base64_encode(b"f"), "Zg==");
    assert_eq!(base64_encode(b"fo"), "Zm8=");
    assert_eq!(base64_encode(b"foo"), "Zm9v");
}

#[test]
fn confirmed_cover_resolves_a_single_noisy_digit_read_without_lowering_consensus() {
    let observations = [
        (300, find("AZE-SUN-INT-QMS-RFI-CIV-1296")),
        (250, find("AZE-SUN-INT-QMS-RFI-CIV-1296")),
        (200, find("AZE-SUN-INT-QMS-RFI-CIV-1296")),
        (350, find("AZE-SUN-INT-QMS-RFI-CIV-1296")),
        (400, find("AZE-SUN-INT-QMS-RFI-CIV-1206")),
    ];
    let cover = confirmed_cover_read(
        "REQUEST FOR INSPECTION INTEC AZE-SUN-INT-QMS-RFI-CIV-1296",
        "test",
    )
    .unwrap();

    assert!(select_rfi_consensus(&observations, None).is_none());
    let recovered = cover_recovery_read(cover, &observations, None).unwrap();
    assert_eq!(recovered.matched.code, "AZE-SUN-INT-QMS-RFI-CIV-1296");
}

#[test]
fn confirmed_cover_does_not_override_stable_or_searchable_conflicts() {
    let observations = [
        (300, find("AZE-SUN-INT-QMS-RFI-CIV-1296")),
        (250, find("AZE-SUN-INT-QMS-RFI-CIV-1296")),
        (200, find("AZE-SUN-INT-QMS-RFI-CIV-1206")),
        (350, find("AZE-SUN-INT-QMS-RFI-CIV-1206")),
    ];
    let cover = confirmed_cover_read(
        "REQUEST FOR INSPECTION INTEC AZE-SUN-INT-QMS-RFI-CIV-1296",
        "test",
    )
    .unwrap();

    assert!(cover_recovery_read(cover, &observations, None).is_none());

    let observations = [
        (300, find("AZE-SUN-INT-QMS-RFI-CIV-1296")),
        (250, find("AZE-SUN-INT-QMS-RFI-CIV-1296")),
        (200, find("AZE-SUN-INT-QMS-RFI-CIV-1296")),
    ];
    let cover = confirmed_cover_read(
        "REQUEST FOR INSPECTION INTEC AZE-SUN-INT-QMS-RFI-CIV-1296",
        "test",
    )
    .unwrap();
    let searchable_conflict = BTreeSet::from(["AZE-SUN-INT-QMS-RFI-CIV-1206".to_string()]);
    assert!(cover_recovery_read(cover, &observations, Some(&searchable_conflict)).is_none());
}

#[test]
fn unconfirmed_cover_identity_cannot_recover_an_unresolved_page() {
    let unresolved = OcrRead {
        matched: find(""),
        source: "ocr:fast-cover:uncertain:test".to_string(),
    };
    assert!(cover_recovery_read(unresolved, &[], None).is_none());
}

#[cfg(windows)]
#[test]
fn tesseract_launch_suppresses_the_windows_console() {
    assert_eq!(tesseract_windows_creation_flags(), 0x0800_0000);
}
