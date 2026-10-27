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
