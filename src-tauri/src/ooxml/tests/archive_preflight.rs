use super::*;

#[test]
fn archive_preflight_rejects_entry_nonzero_disk_number_start() {
    assert_archive_preflight_rejects(mutate_first_central_field(
        ordered_fixture(),
        34,
        &1_u16.to_le_bytes(),
    ));
}

#[test]
fn archive_preflight_rejects_entry_zip64_central_sentinels() {
    for field_offset in [20, 24, 42] {
        assert_archive_preflight_rejects(mutate_first_central_field(
            ordered_fixture(),
            field_offset,
            &u32::MAX.to_le_bytes(),
        ));
    }
    assert_archive_preflight_rejects(mutate_first_central_field(
        ordered_fixture(),
        34,
        &u16::MAX.to_le_bytes(),
    ));
}

#[test]
fn archive_preflight_rejects_entry_zip64_central_extra_without_sentinel() {
    assert_archive_preflight_rejects(insert_first_central_extra(
        ordered_fixture(),
        &[0x01, 0x00, 0x00, 0x00],
    ));
}

#[test]
fn archive_preflight_rejects_entry_malformed_central_extra() {
    assert_archive_preflight_rejects(insert_first_central_extra(
        ordered_fixture(),
        &[0xfe, 0xca, 0x04, 0x00],
    ));
}

#[test]
fn archive_preflight_rejects_entry_zip64_local_size_sentinels() {
    for field_offset in [18, 22] {
        assert_archive_preflight_rejects(mutate_first_local_field(
            ordered_fixture(),
            field_offset,
            &u32::MAX.to_le_bytes(),
        ));
    }
}

#[test]
fn archive_preflight_rejects_entry_zip64_local_extra() {
    assert_archive_preflight_rejects(insert_last_local_extra(
        ordered_fixture(),
        &[0x01, 0x00, 0x00, 0x00],
    ));
}

#[test]
fn archive_preflight_rejects_entry_malformed_local_extra() {
    assert_archive_preflight_rejects(insert_last_local_extra(
        ordered_fixture(),
        &[0xfe, 0xca, 0x04, 0x00],
    ));
}

#[test]
fn ordered_worksheet_discovery_rejects_oversized_source_before_zip_parse() {
    let error = ordered_workbook_discovery(&mut SparseReader {
        len: super::super::MAX_PACKAGE_BYTES + 1,
        position: 0,
    })
    .unwrap_err();

    assert_eq!(error.to_string(), "workbook source exceeded the size limit");
}

#[test]
fn ordered_worksheet_discovery_rejects_declared_entry_count_before_zip_parse() {
    let error = ordered_workbook_discovery(&mut declared_entry_count_fixture(
        u16::try_from(super::super::MAX_ENTRIES + 1).unwrap(),
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "workbook ZIP entry count is invalid");
}

#[test]
fn ordered_worksheet_discovery_rejects_decoy_eocd_before_parser_fallback() {
    let error = ordered_workbook_discovery(&mut append_decoy_eocd(ordered_fixture())).unwrap_err();

    assert_eq!(error.to_string(), "workbook ZIP metadata is invalid");
}

#[test]
fn ordered_worksheet_discovery_rejects_inflated_final_eocd_before_parser_fallback() {
    let error = ordered_workbook_discovery(&mut append_parser_fallback_eocd(ordered_fixture()))
        .unwrap_err();

    assert_eq!(error.to_string(), "workbook ZIP metadata is invalid");
}

#[test]
fn ordered_worksheet_discovery_rejects_semantically_invalid_final_directory() {
    let error =
        ordered_workbook_discovery(&mut append_semantically_invalid_directory(ordered_fixture()))
            .unwrap_err();

    assert_eq!(error.to_string(), "workbook ZIP metadata is invalid");
}
