use std::io::{Read, Write};

use tempfile::tempdir;

use super::*;

fn normalized_source() -> String {
    [
        include_str!("../cap_fs.rs"),
        include_str!("anchored.rs"),
        include_str!("unix_dir.rs"),
        include_str!("windows_dir.rs"),
    ]
    .join("\n")
    .replace("\r\n", "\n")
        + "\n#[cfg(test)]\nmod tests"
}

#[test]
fn secured_create_anchored_create_new_no_clobber() {
    let fixture = tempdir().unwrap();
    let root = CapabilityRoot::open(fixture.path()).unwrap();
    let path = fixture.path().join("created.xlsx");
    let anchored = root.authorize(&path).unwrap().anchor().unwrap();

    let mut created = anchored.create_new().unwrap();
    created.write_all(b"owned bytes").unwrap();
    created.sync_all().unwrap();
    drop(created);

    let error = anchored.create_new().unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    let mut contents = Vec::new();
    anchored
        .open_read()
        .unwrap()
        .read_to_end(&mut contents)
        .unwrap();
    assert_eq!(contents, b"owned bytes");
}

#[test]
fn retained_publication_retain_current_name_is_same_parent_and_no_clobber() {
    let fixture = tempdir().unwrap();
    let root = CapabilityRoot::open(fixture.path()).unwrap();
    let canonical_path = fixture.path().join("Follow Up.xlsx");
    let retained_path = fixture
        .path()
        .join(".docflow-s5-retained-original-test.xlsx");
    std::fs::write(&canonical_path, b"original").unwrap();
    let canonical = root.authorize(&canonical_path).unwrap().anchor().unwrap();
    let retained = root.authorize(&retained_path).unwrap().anchor().unwrap();

    canonical.retain_current_name_no_clobber(&retained).unwrap();

    assert!(!canonical_path.exists());
    assert_eq!(std::fs::read(&retained_path).unwrap(), b"original");

    std::fs::write(&canonical_path, b"racer").unwrap();
    let error = canonical
        .retain_current_name_no_clobber(&retained)
        .unwrap_err();
    assert!(!error.is_outcome_unknown());
    assert_eq!(std::fs::read(&canonical_path).unwrap(), b"racer");
    assert_eq!(std::fs::read(&retained_path).unwrap(), b"original");
}

#[test]
fn failed_apply_restore_moves_only_the_same_parent_retained_original_without_clobber() {
    let fixture = tempdir().unwrap();
    let root = CapabilityRoot::open(fixture.path()).unwrap();
    let canonical_path = fixture.path().join("Follow Up.xlsx");
    let retained_path = fixture
        .path()
        .join(".docflow-s5-retained-original-test.xlsx");
    std::fs::write(&canonical_path, b"original").unwrap();
    let canonical = root.authorize(&canonical_path).unwrap().anchor().unwrap();
    let retained = root.authorize(&retained_path).unwrap().anchor().unwrap();
    let original_identity = file_identity(&canonical.open_read().unwrap()).unwrap();

    canonical.retain_current_name_no_clobber(&retained).unwrap();
    retained
        .restore_original_name_no_clobber(&canonical)
        .unwrap();

    assert_eq!(
        file_identity(&canonical.open_read().unwrap()).unwrap(),
        original_identity
    );
    assert_eq!(std::fs::read(&canonical_path).unwrap(), b"original");
    assert!(!retained_path.exists());

    canonical.retain_current_name_no_clobber(&retained).unwrap();
    std::fs::write(&canonical_path, b"racer").unwrap();
    let error = retained
        .restore_original_name_no_clobber(&canonical)
        .unwrap_err();
    assert!(!error.is_outcome_unknown());
    assert_eq!(std::fs::read(&canonical_path).unwrap(), b"racer");
    assert_eq!(std::fs::read(&retained_path).unwrap(), b"original");
}

#[test]
fn directory_access_and_retained_quota_read_only_root_rejects_mutation() {
    let fixture = tempdir().unwrap();
    let read_only =
        CapabilityRoot::open_with_access(fixture.path(), DirectoryAccess::ReadOnly).unwrap();
    let denied_path = fixture.path().join("denied.xlsx");
    let denied = read_only.authorize(&denied_path).unwrap().anchor().unwrap();
    assert_eq!(
        denied.create_new().unwrap_err().kind(),
        std::io::ErrorKind::PermissionDenied
    );
    let protected_path = fixture.path().join("protected.xlsx");
    std::fs::write(&protected_path, b"must remain").unwrap();
    let protected = read_only
        .authorize(&protected_path)
        .unwrap()
        .anchor()
        .unwrap();
    assert_eq!(
        protected.remove_file().unwrap_err().kind(),
        std::io::ErrorKind::PermissionDenied
    );
    assert_eq!(std::fs::read(&protected_path).unwrap(), b"must remain");

    let read_write =
        CapabilityRoot::open_with_access(fixture.path(), DirectoryAccess::ReadWrite).unwrap();
    let allowed_path = fixture.path().join("allowed.xlsx");
    read_write
        .authorize(&allowed_path)
        .unwrap()
        .anchor()
        .unwrap()
        .create_new()
        .unwrap();

    let legacy_path = fixture.path().join("legacy.xlsx");
    CapabilityRoot::open(fixture.path())
        .unwrap()
        .authorize(&legacy_path)
        .unwrap()
        .anchor()
        .unwrap()
        .create_new()
        .unwrap();
}

#[test]
fn directory_access_and_retained_quota_artifact_parent_is_typed_and_no_clobber() {
    let fixture = tempdir().unwrap();
    let root = CapabilityRoot::open(fixture.path()).unwrap();
    let artifact_path = fixture.path().join(".docflow-backups");
    let artifact = root.authorize(&artifact_path).unwrap();

    let created = artifact.create_artifact_parent_no_clobber().unwrap();
    assert!(matches!(
        created,
        ArtifactParentCreateOutcome::Created { .. }
    ));
    assert!(artifact_path.is_dir());

    let existing = artifact.create_artifact_parent_no_clobber().unwrap();
    assert_eq!(existing, ArtifactParentCreateOutcome::AlreadyExists);
}

#[cfg(not(windows))]
#[test]
fn secured_create_identified_retains_live_handle_when_post_create_identity_is_unknown() {
    let fixture = tempdir().unwrap();
    let root = CapabilityRoot::open(fixture.path()).unwrap();
    let path = fixture.path().join("unknown-identity.xlsx");
    let anchored = root.authorize(&path).unwrap().anchor().unwrap();
    let descriptor = super::CreationSecurityDescriptor::current_owner_only().unwrap();
    super::inject_post_create_identity_failure_once();

    let SecuredCreateOutcome::CreatedIdentityUnknown { mut file } =
        anchored.create_new_secured_identified(&descriptor).unwrap()
    else {
        panic!("injected identity failure must preserve the live handle");
    };
    file.write_all(b"retained").unwrap();
    file.sync_all().unwrap();
    assert_eq!(std::fs::read(path).unwrap(), b"retained");
}

#[test]
fn pinned_overwrite_verifies_current_handle_before_restoring_exact_source_bytes() {
    let fixture = tempdir().unwrap();
    let root = CapabilityRoot::open(fixture.path()).unwrap();
    let backup_path = fixture.path().join("backup.xlsx");
    let workbook_path = fixture.path().join("workbook.xlsx");
    std::fs::write(&backup_path, b"original workbook").unwrap();
    std::fs::write(&workbook_path, b"applied workbook").unwrap();
    let backup = root.authorize(&backup_path).unwrap().anchor().unwrap();
    let workbook = root.authorize(&workbook_path).unwrap().anchor().unwrap();
    let applied_hash = workbook.sha256().unwrap();

    workbook
        .overwrite_verified_from(&backup, &applied_hash)
        .unwrap();
    assert_eq!(std::fs::read(&workbook_path).unwrap(), b"original workbook");

    std::fs::write(&workbook_path, b"later edit").unwrap();
    let error = workbook
        .overwrite_verified_from(&backup, &applied_hash)
        .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(std::fs::read(&workbook_path).unwrap(), b"later edit");
}

#[test]
fn pinned_remove_verifies_the_same_handle_before_deletion() {
    let fixture = tempdir().unwrap();
    let root = CapabilityRoot::open(fixture.path()).unwrap();
    let path = fixture.path().join("owned-backup.xlsx");
    std::fs::write(&path, b"verified backup").unwrap();
    let anchored = root.authorize(&path).unwrap().anchor().unwrap();
    let expected = anchored.sha256().unwrap();

    let error = anchored
        .remove_verified("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
        .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(path.exists());

    anchored.remove_verified(&expected).unwrap();
    assert!(!path.exists());
}

#[test]
fn backup_stream_skip_unnamed_default_payload_is_excluded_from_auxiliary_budget() {
    let default_bytes = 4_u64 * 1024 * 1024 * 1024;
    let charge = super::windows_backup_stream_auxiliary_charge(&[
        (1, "", default_bytes),
        (1, ":audit:$DATA", 17),
        (3, "", 23),
    ])
    .unwrap();

    assert_eq!(charge, 17 + ":audit:$DATA".len() as u64 + 16 + 23 + 16);
}

#[test]
fn directory_access_and_retained_quota_apply_reservation_charges_three_auxiliary_copies() {
    let reservation = RetainedStorageReservation::for_apply(100, 20).unwrap();
    assert_eq!(reservation.file_count, 3);
    assert_eq!(
        reservation.logical_charge_bytes,
        crate::ooxml::MAX_PACKAGE_BYTES + 260
    );
}

#[test]
fn directory_access_and_retained_quota_scan_counts_only_bounded_s5_artifacts() {
    let fixture = tempdir().unwrap();
    let mut expected_charge = 0_u64;
    for (name, bytes) in [
        (".docflow-s5-backup-a.xlsx", b"a".as_slice()),
        (".docflow-s5-private-output-a.xlsx", b"bb".as_slice()),
        (".docflow-s5-retained-original-a.xlsx", b"ccc".as_slice()),
        (".docflow-s5-retained-applied-a.xlsx", b"dddd".as_slice()),
        ("ordinary.xlsx", b"not retained".as_slice()),
    ] {
        let path = fixture.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        if name.starts_with(".docflow-s5-") {
            let file = std::fs::File::open(path).unwrap();
            expected_charge += bytes.len() as u64
                + FileSecuritySnapshot::capture(&file, None)
                    .unwrap()
                    .auxiliary_charge_bytes();
        }
    }
    let root = CapabilityRoot::open(fixture.path()).unwrap();
    let authorized = root.authorize_directory(fixture.path()).unwrap();

    let usage = authorized.s5_retained_storage_usage(4).unwrap();
    assert_eq!(usage.file_count, 4);
    assert_eq!(usage.logical_charge_bytes, expected_charge);
    assert!(authorized.s5_retained_storage_usage(3).is_err());
}

#[test]
fn directory_access_and_retained_quota_undo_reservation_charges_retained_applied_auxiliary() {
    let reservation = RetainedStorageReservation::for_undo(100, 20).unwrap();
    assert_eq!(reservation.file_count, 1);
    assert_eq!(reservation.logical_charge_bytes, 120);
}

#[cfg(unix)]
#[test]
fn file_security_snapshot_round_trips_mode_and_complete_digest() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = tempdir().unwrap();
    let root = CapabilityRoot::open(fixture.path()).unwrap();
    let source_path = fixture.path().join("source.xlsx");
    std::fs::write(&source_path, b"source bytes").unwrap();
    std::fs::set_permissions(&source_path, std::fs::Permissions::from_mode(0o640)).unwrap();
    let source = root.authorize(&source_path).unwrap().anchor().unwrap();
    let source_file = source.open_read().unwrap();
    #[cfg(target_os = "macos")]
    set_test_xattr(&source_file, "user.docflow", b"retained metadata");
    let snapshot = FileSecuritySnapshot::capture(&source_file, None).unwrap();
    let destination_path = fixture.path().join("destination.xlsx");
    let destination = root.authorize(&destination_path).unwrap().anchor().unwrap();
    let parent = destination.capture_parent_security().unwrap();
    let descriptor = snapshot.creation_descriptor(&parent).unwrap();

    let mut created = destination.create_new_secured(&descriptor).unwrap();
    snapshot.verify_created_access(&created).unwrap();
    snapshot.replay_and_verify_auxiliary(&mut created).unwrap();
    created.write_all(b"source bytes").unwrap();
    created.sync_all().unwrap();
    snapshot.verify_complete(&created).unwrap();

    assert_eq!(
        std::fs::metadata(&destination_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    assert_eq!(snapshot.sha256().len(), 64);
    assert_eq!(snapshot.access_sha256().len(), 64);
    assert_eq!(snapshot.auxiliary_sha256().len(), 64);
    #[cfg(target_os = "macos")]
    assert_eq!(
        get_test_xattr(&created.0, "user.docflow"),
        b"retained metadata"
    );
}

#[cfg(target_os = "macos")]
fn set_test_xattr(file: &std::fs::File, name: &str, value: &[u8]) {
    use std::{ffi::CString, os::fd::AsRawFd};

    let name = CString::new(name).unwrap();
    let result = unsafe {
        libc::fsetxattr(
            file.as_raw_fd(),
            name.as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
            0,
        )
    };
    assert_eq!(result, 0, "{}", std::io::Error::last_os_error());
}

#[cfg(target_os = "macos")]
fn get_test_xattr(file: &std::fs::File, name: &str) -> Vec<u8> {
    use std::{ffi::CString, os::fd::AsRawFd};

    let name = CString::new(name).unwrap();
    let size = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            name.as_ptr(),
            std::ptr::null_mut(),
            0,
            0,
            0,
        )
    };
    assert!(size >= 0, "{}", std::io::Error::last_os_error());
    let mut value = vec![0_u8; size as usize];
    let read = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            name.as_ptr(),
            value.as_mut_ptr().cast(),
            value.len(),
            0,
            0,
        )
    };
    assert_eq!(read, size, "{}", std::io::Error::last_os_error());
    value
}

#[test]
fn windows_rename_uses_the_native_root_relative_information_contract() {
    let source = normalized_source();
    let rename = source
        .split("fn rename_open_handle(")
        .nth(1)
        .expect("Windows rename implementation must remain inspectable")
        .split("fn delete_open_handle(")
        .next()
        .expect("Windows rename implementation must remain bounded");

    assert!(rename.contains("NtSetInformationFile"));
    assert!(rename.contains("FILE_RENAME_INFORMATION"));
    assert!(rename.contains("FileRenameInformation"));
    assert!(rename.contains("RootDirectory"));
    assert!(rename.contains("destination_parent: &File"));
    assert!(source.contains("fn duplicate_rename_target_directory"));
    assert!(source.contains("DuplicateHandle"));
    assert!(!rename.contains("SetFileInformationByHandle"));
}

#[cfg(windows)]
#[test]
fn windows_cross_parent_rename_is_capability_relative_and_no_clobber() {
    let fixture = tempdir().unwrap();
    let source_dir = fixture.path().join("source");
    let destination_dir = fixture.path().join("destination");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&destination_dir).unwrap();
    let source_path = source_dir.join("source.txt");
    let destination_path = destination_dir.join("destination.txt");
    std::fs::write(&source_path, b"capability bytes").unwrap();
    let root = CapabilityRoot::open(fixture.path()).unwrap();
    let source = root.authorize(&source_path).unwrap().anchor().unwrap();
    let destination = root.authorize(&destination_path).unwrap().anchor().unwrap();

    source.move_no_clobber_to(&destination).unwrap();

    assert!(!source_path.exists());
    assert_eq!(
        std::fs::read(destination_path).unwrap(),
        b"capability bytes"
    );
}

#[test]
fn windows_zone_identifier_open_is_identity_checked_and_root_relative() {
    let source = normalized_source();
    let source = source
        .rsplit_once("#[cfg(test)]\nmod tests")
        .expect("production capability source")
        .0;
    let anchored = source
        .split("pub(crate) fn open_zone_identifier(")
        .nth(1)
        .expect("AnchoredPath must expose one bounded Zone.Identifier reader")
        .split("pub(crate) fn identity(")
        .next()
        .expect("Zone.Identifier reader must remain in the anchored API");
    assert!(anchored.contains("self.open_read()?"));
    assert!(anchored.contains("self.parent.open_zone_identifier_stream(&self.name)"));
    let identity_open = anchored.find("self.open_read()?").unwrap();
    let stream_open = anchored
        .find("self.parent.open_zone_identifier_stream(&self.name)")
        .unwrap();
    let identity_close = anchored.find("drop(verified_source)").unwrap();
    assert!(identity_open < stream_open);
    assert!(stream_open < identity_close);

    let windows_directory = source
        .split("fn open_zone_identifier_stream(")
        .nth(1)
        .expect("Windows directory capability must own ADS opening")
        .split("fn create_file_new(")
        .next()
        .expect("ADS opening must remain bounded");
    assert!(windows_directory.contains("nt_open_relative("));
    assert!(windows_directory.contains("Zone.Identifier:$DATA"));
    assert!(!windows_directory.contains("self.canonical"));
    assert!(!windows_directory.contains("std::fs::File::open"));
}

#[test]
fn windows_directory_sync_uses_flush_file_buffers() {
    let source = normalized_source();
    let windows = source
        .split("#[cfg(windows)]\nimpl PlatformDirectory")
        .nth(1)
        .expect("Windows directory implementation")
        .split("#[cfg(windows)]\n#[derive(Clone, Copy)]")
        .next()
        .expect("bounded Windows directory implementation");
    let sync = windows
        .split("fn sync_all(&self)")
        .nth(1)
        .expect("Windows directory sync")
        .split("}\n}")
        .next()
        .expect("bounded Windows directory sync");
    assert!(sync.contains("injected_directory_sync_failure()?"));
    assert!(sync.contains("FlushFileBuffers"));
    assert!(sync.contains("io::Error::last_os_error()"));
    assert!(
        sync.find("injected_directory_sync_failure()?").unwrap()
            < sync.find("FlushFileBuffers").unwrap()
    );
}

#[test]
fn windows_capability_handles_share_delete_for_handle_relative_renames() {
    let source = normalized_source();
    let windows = source
        .split("#[cfg(windows)]\nimpl PlatformDirectory")
        .nth(1)
        .expect("Windows directory implementation");
    let ambient_open = windows
        .split("fn open_ambient")
        .nth(1)
        .and_then(|value| value.split("fn try_clone").next())
        .expect("ambient directory open");
    let relative_open = windows
        .split("fn nt_open_relative")
        .nth(1)
        .and_then(|value| value.split("fn rename_open_handle").next())
        .expect("relative NT open");

    assert!(ambient_open.contains("FILE_SHARE_DELETE"));
    assert!(relative_open.contains("FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE"));
}

#[test]
fn windows_sharing_violation_retry_is_bounded_and_error_exact() {
    let mut attempts = 0;
    super::retry_windows_sharing_violation(|| {
        attempts += 1;
        if attempts < 3 {
            Err(std::io::Error::from_raw_os_error(32))
        } else {
            Ok(())
        }
    })
    .unwrap();
    assert_eq!(attempts, 3);

    let mut denied_attempts = 0;
    let denied = super::retry_windows_sharing_violation(|| {
        denied_attempts += 1;
        Err(std::io::Error::from_raw_os_error(5))
    })
    .unwrap_err();
    assert_eq!(denied.raw_os_error(), Some(5));
    assert_eq!(denied_attempts, 1);
}

#[test]
fn contextual_io_errors_keep_retryable_windows_codes() {
    let error = super::preserve_os_error(
        "commit relative rename",
        std::io::Error::from_raw_os_error(32),
    );
    assert_eq!(error.raw_os_error(), Some(32));
}

#[test]
fn windows_copy_no_clobber_uses_the_owned_destination_handle_without_rename() {
    let source = normalized_source();
    let copy = source
        .split("pub(crate) fn copy_no_clobber_from_file")
        .nth(1)
        .and_then(|value| value.split("pub(crate) fn write_no_clobber").next())
        .expect("bounded copy implementation");
    let windows = copy
        .split("#[cfg(windows)]")
        .nth(1)
        .and_then(|value| value.split("#[cfg(not(windows))]").next())
        .expect("Windows copy branch");

    assert!(windows.contains("create_file_new"));
    assert!(windows.contains("output.sync_all()?"));
    assert!(!windows.contains("rename("));
}

#[test]
fn windows_secured_creation_applies_attributes_through_the_owned_handle() {
    let source = normalized_source();
    let function = source
        .split("fn apply_creation_security")
        .nth(1)
        .expect("creation security function")
        .split("#[cfg(unix)]\nfn capture_unix_xattrs")
        .next()
        .expect("Windows creation security implementation");
    assert!(function.contains("SetFileInformationByHandle"));
    assert!(function.contains("FileBasicInfo"));
    assert!(!function.contains("requires the secured NtCreateFile path"));
}
