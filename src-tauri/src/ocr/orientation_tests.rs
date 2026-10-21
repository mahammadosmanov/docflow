use super::*;

#[test]
fn parses_tesseract_osd_rotation_and_confidence() {
    let result = parse_orientation_output(
        "Page number: 0\nOrientation in degrees: 270\nRotate: 90\nOrientation confidence: 18.42\nScript: Latin\n",
        250,
    )
    .unwrap();
    assert_eq!(result.rotate_clockwise, 90);
    assert_eq!(result.confidence, 18.42);
}

#[test]
fn rejects_missing_or_non_right_angle_osd_results() {
    assert!(parse_orientation_output("Rotate: 45\nOrientation confidence: 20\n", 250).is_err());
    assert!(parse_orientation_output("Rotate: 90\n", 250).is_err());
}

#[test]
fn accepts_only_clear_four_direction_ocr_winners() {
    let scores = [
        DirectionScore::new(0, 104, 4_999.0),
        DirectionScore::new(90, 288, 20_606.4),
        DirectionScore::new(180, 207, 10_069.5),
        DirectionScore::new(270, 106, 5_492.2),
    ];
    let selected = select_direction_score(&scores, 250).expect("clear 90 degree winner");
    assert_eq!(selected.rotate_clockwise, 90);
    assert!(selected.confidence >= 10.0);
    assert!(selected.confidence <= 100.0);
}

#[test]
fn rejects_sparse_or_near_tied_direction_scores() {
    let sparse = [
        DirectionScore::new(0, 17, 915.2),
        DirectionScore::new(90, 11, 468.6),
        DirectionScore::new(180, 23, 1_146.9),
        DirectionScore::new(270, 4, 182.6),
    ];
    assert!(select_direction_score(&sparse, 250).is_none());

    let near_tie = [
        DirectionScore::new(0, 219, 10_429.9),
        DirectionScore::new(90, 13, 719.5),
        DirectionScore::new(180, 206, 10_181.0),
        DirectionScore::new(270, 13, 678.7),
    ];
    assert!(select_direction_score(&near_tie, 250).is_none());
}

#[test]
fn page_header_consensus_resolves_conflicting_drawing_text_directions() {
    let primary = [
        DirectionScore::new(0, 14, 767.0).with_header(5, 218.0),
        DirectionScore::new(90, 11, 604.0).with_header(1, 45.0),
        DirectionScore::new(180, 4, 100.0),
        DirectionScore::new(270, 3, 90.0),
    ];
    let secondary = [
        DirectionScore::new(0, 8, 434.0).with_header(4, 180.0),
        DirectionScore::new(90, 10, 640.0).with_header(1, 48.0),
        DirectionScore::new(180, 9, 448.0),
        DirectionScore::new(270, 3, 100.0),
    ];
    assert_eq!(
        select_correlated_header_score(&primary, &secondary)
            .expect("both DPIs identify the same page-header direction")
            .rotate_clockwise,
        0
    );

    let conflicting_header = [
        DirectionScore::new(0, 8, 434.0),
        DirectionScore::new(90, 10, 640.0).with_header(4, 190.0),
        DirectionScore::new(180, 9, 448.0),
        DirectionScore::new(270, 3, 100.0),
    ];
    assert!(select_correlated_header_score(&primary, &conflicting_header).is_none());
}

#[test]
fn strict_rfi_header_identity_resolves_single_token_native_orientation() {
    const TSV_HEADER: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n";
    let primary_code = parse_direction_score_tsv(
        &format!(
            "{TSV_HEADER}5\t1\t1\t1\t1\t1\t912\t186\t1082\t74\t70.969269\tAZE-SUN-INT-QMS-RFI-CIV-1158\n"
        ),
        270,
        1_838,
    );
    let secondary_code = parse_direction_score_tsv(
        &format!(
            "{TSV_HEADER}5\t1\t1\t1\t1\t1\t1228\t252\t1456\t99\t88.430008\tAZE-SUN-INT-QMS-RFI-CIV-1158\n"
        ),
        270,
        2_476,
    );
    assert_eq!(
        primary_code
            .header_rfi_codes
            .get("AZE-SUN-INT-QMS-RFI-CIV-1158"),
        Some(&70.969269)
    );
    assert_eq!(
        secondary_code
            .header_rfi_codes
            .get("AZE-SUN-INT-QMS-RFI-CIV-1158"),
        Some(&88.430008)
    );

    let primary = [
        DirectionScore::new(0, 28, 1_811.0),
        DirectionScore::new(90, 0, 0.0),
        DirectionScore::new(180, 0, 0.0),
        primary_code,
    ];
    let secondary = [
        DirectionScore::new(0, 55, 2_678.0),
        DirectionScore::new(90, 0, 0.0),
        DirectionScore::new(180, 52, 2_423.0),
        secondary_code,
    ];
    assert_eq!(
        select_correlated_header_score(&primary, &secondary)
            .expect("one strict RFI identity at the same header angle on both DPI")
            .rotate_clockwise,
        270
    );

    let different_angle = [
        DirectionScore::new(0, 0, 0.0),
        parse_direction_score_tsv(
            &format!(
                "{TSV_HEADER}5\t1\t1\t1\t1\t1\t1228\t252\t1456\t99\t88.430008\tAZE-SUN-INT-QMS-RFI-CIV-1158\n"
            ),
            90,
            2_476,
        ),
        DirectionScore::new(180, 0, 0.0),
        DirectionScore::new(270, 0, 0.0),
    ];
    assert!(
        select_correlated_header_score(&primary, &different_angle).is_none(),
        "cross-resolution direction conflict must remain uncertain"
    );

    let competing_code = [
        DirectionScore::new(0, 0, 0.0),
        parse_direction_score_tsv(
            &format!(
                "{TSV_HEADER}5\t1\t1\t1\t1\t1\t1228\t252\t1456\t99\t88.430008\tAZE-SUN-INT-QMS-RFI-MEC-0772\n"
            ),
            90,
            2_476,
        ),
        DirectionScore::new(180, 0, 0.0),
        secondary[3].clone(),
    ];
    assert!(
        select_correlated_header_score(&primary, &competing_code).is_none(),
        "a second exact RFI code at another direction must veto correction"
    );

    let outside_header = parse_direction_score_tsv(
        &format!(
            "{TSV_HEADER}5\t1\t1\t1\t1\t1\t912\t400\t1082\t74\t70.969269\tAZE-SUN-INT-QMS-RFI-CIV-1158\n"
        ),
        270,
        1_838,
    );
    assert!(outside_header.header_rfi_codes.is_empty());
    let low_confidence = parse_direction_score_tsv(
        &format!(
            "{TSV_HEADER}5\t1\t1\t1\t1\t1\t912\t186\t1082\t74\t59.999\tAZE-SUN-INT-QMS-RFI-CIV-1158\n"
        ),
        270,
        1_838,
    );
    assert!(low_confidence.header_rfi_codes.is_empty());
}

#[test]
fn rfi_header_identity_rejects_noncanonical_and_competing_codes() {
    const TSV_HEADER: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n";
    let invalid_suffix = parse_direction_score_tsv(
        &format!(
            "{TSV_HEADER}5\t1\t1\t1\t1\t1\t912\t186\t1082\t74\t90.0\tAZE-SUN-INT-QMS-RFI-CIV-1158A\n"
        ),
        270,
        1_838,
    );
    assert!(invalid_suffix.header_rfi_codes.is_empty());

    let competing = parse_direction_score_tsv(
        &format!(
            "{TSV_HEADER}5\t1\t1\t1\t1\t1\t912\t186\t1082\t74\t90.0\tAZE-SUN-INT-QMS-RFI-CIV-1158\n5\t1\t1\t1\t1\t1\t1200\t186\t500\t74\t90.0\tAZE-SUN-INT-QMS-RFI-MEC-0772\n"
        ),
        270,
        1_838,
    );
    assert_eq!(competing.header_rfi_codes.len(), 2);
    let one_code = parse_direction_score_tsv(
        &format!(
            "{TSV_HEADER}5\t1\t1\t1\t1\t1\t912\t186\t1082\t74\t90.0\tAZE-SUN-INT-QMS-RFI-CIV-1158\n"
        ),
        270,
        2_476,
    );
    let primary = [
        DirectionScore::new(0, 0, 0.0),
        DirectionScore::new(90, 0, 0.0),
        DirectionScore::new(180, 0, 0.0),
        competing,
    ];
    let secondary = [
        DirectionScore::new(0, 0, 0.0),
        DirectionScore::new(90, 0, 0.0),
        DirectionScore::new(180, 0, 0.0),
        one_code,
    ];
    let generic_header_only = [
        DirectionScore::new(0, 14, 767.0).with_header(5, 218.0),
        DirectionScore::new(90, 0, 0.0),
        DirectionScore::new(180, 0, 0.0),
        DirectionScore::new(270, 0, 0.0),
    ];
    assert!(
        select_correlated_header_score(&generic_header_only, &secondary).is_none(),
        "one-DPI exact identity evidence must not fall back to generic text"
    );
    assert!(select_correlated_header_score(&primary, &secondary).is_none());
}

#[test]
fn weak_cover_needs_matching_osd_and_ocr_at_both_resolutions() {
    let primary = [
        DirectionScore::new(0, 21, 1_287.0),
        DirectionScore::new(90, 20, 1_029.0),
        DirectionScore::new(180, 13, 600.0),
        DirectionScore::new(270, 12, 550.0),
    ];
    let secondary = [
        DirectionScore::new(0, 52, 2_586.0),
        DirectionScore::new(90, 20, 900.0),
        DirectionScore::new(180, 56, 2_377.0),
        DirectionScore::new(270, 19, 800.0),
    ];
    let osd = OrientationDetection {
        rotate_clockwise: 0,
        confidence: 1.8,
        method: "osd".to_string(),
    };
    assert!(select_direction_score(&secondary, 350).is_none());
    assert_eq!(
        select_correlated_direction_score(&primary, &secondary, Some(&osd), Some(&osd))
            .unwrap()
            .rotate_clockwise,
        0
    );
    let conflicting_osd = OrientationDetection {
        rotate_clockwise: 180,
        ..osd.clone()
    };
    assert!(
        select_correlated_direction_score(&primary, &secondary, Some(&osd), Some(&conflicting_osd))
            .is_none()
    );
    let conflicting_ocr = [
        secondary[0].clone(),
        secondary[1].clone(),
        DirectionScore::new(180, 80, 5_000.0),
        secondary[3].clone(),
    ];
    assert!(
        select_correlated_direction_score(&primary, &conflicting_ocr, Some(&osd), Some(&osd))
            .is_none()
    );
    let sparse = [
        DirectionScore::new(0, 10, 600.0),
        DirectionScore::new(90, 3, 90.0),
        DirectionScore::new(180, 4, 160.0),
        DirectionScore::new(270, 3, 100.0),
    ];
    assert!(
        select_correlated_direction_score(&sparse, &secondary, Some(&osd), Some(&osd)).is_none()
    );
    let near_tie = [
        DirectionScore::new(0, 52, 2_400.0),
        secondary[1].clone(),
        DirectionScore::new(180, 56, 2_350.0),
        secondary[3].clone(),
    ];
    assert!(
        select_correlated_direction_score(&primary, &near_tie, Some(&osd), Some(&osd)).is_none()
    );
}

#[test]
fn combines_only_agreeing_osd_results() {
    let first = OrientationDetection {
        rotate_clockwise: 0,
        confidence: 7.1,
        method: "first".to_string(),
    };
    let second = OrientationDetection {
        rotate_clockwise: 0,
        confidence: 7.8,
        method: "second".to_string(),
    };
    let selected = select_osd_detection(Some(&first), Some(&second))
        .expect("agreeing OSD passes should combine");
    assert_eq!(selected.rotate_clockwise, 0);
    assert!(selected.confidence >= 10.0);

    let conflict = OrientationDetection {
        rotate_clockwise: 180,
        confidence: 8.0,
        method: "conflict".to_string(),
    };
    assert!(select_osd_detection(Some(&first), Some(&conflict)).is_none());
}

#[test]
fn rotates_grayscale_pixels_clockwise_without_mirroring() {
    let image = GrayImage {
        width: 2,
        height: 3,
        pixels: vec![1, 2, 3, 4, 5, 6],
    };
    let rotated = rotate_gray_clockwise(&image, 90).unwrap();
    assert_eq!((rotated.width, rotated.height), (3, 2));
    assert_eq!(rotated.pixels, vec![5, 3, 1, 6, 4, 2]);
    assert_eq!(
        rotate_gray_clockwise(&image, 180).unwrap().pixels,
        vec![6, 5, 4, 3, 2, 1]
    );
}
