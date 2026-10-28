use super::*;

#[test]
fn rfi_print_finder_issues_exact_batch_print_file_selections() {
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);
    let root = temporary.path().join("RFI System");
    let code = "AZE-SUN-INT-QMS-RFI-CIV-1319";
    let duplicate = root.join("CIV/old").join(format!("{code}.docx"));
    let first = root.join("CIV").join(format!("{code}.docx"));
    let sketch = root.join("CIV").join(format!("{code} Sketch.pdf"));
    for path in [&duplicate, &first, &sketch] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"").unwrap();
    }
    let caller = backend.caller_identity().unwrap();
    let root_selection = backend
        .selections
        .issue(
            &caller,
            "files.batch_print",
            "rfi_system_root",
            &root,
            SelectionKind::Folder,
        )
        .unwrap();
    let result = backend
        .find_rfi_print_files(json!({
            "root": root_selection,
            "numbers": ["CIV-1319"],
            "firstPage": true,
            "sketch": true,
        }))
        .unwrap();
    let candidates = result["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 3);
    let first_pages = candidates
        .iter()
        .filter(|candidate| candidate["documentType"] == "firstPage")
        .collect::<Vec<_>>();
    let sketches = candidates
        .iter()
        .filter(|candidate| candidate["documentType"] == "sketch")
        .collect::<Vec<_>>();
    assert_eq!(first_pages.len(), 2);
    assert!(
        first_pages
            .iter()
            .all(|candidate| candidate["selected"] == false)
    );
    assert_eq!(sketches.len(), 1);
    assert_eq!(sketches[0]["selected"], true);
    for candidate in candidates {
        let selection: PathSelection =
            serde_json::from_value(candidate["selection"].clone()).unwrap();
        let path = backend
            .selections
            .resolve(
                &selection,
                &caller,
                "files.batch_print",
                "files",
                SelectionKind::File,
            )
            .unwrap();
        assert!(path.starts_with(fs::canonicalize(&root).unwrap()));
    }
}

#[test]
fn finder_revokes_an_issued_selection_outside_the_selected_root() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("selected-root");
    fs::create_dir(&root).unwrap();
    let outside = temporary.path().join("outside.pdf");
    fs::write(&outside, b"outside").unwrap();
    let caller = CallerIdentity::new("owner", "session");
    let selections = PathSelectionStore::new(Duration::from_secs(60));
    let selection = selections
        .issue(
            &caller,
            "files.batch_print",
            "files",
            &outside,
            SelectionKind::File,
        )
        .unwrap();

    let error = resolve_scoped_rfi_print_file(
        &selections,
        &selection,
        &caller,
        &fs::canonicalize(root).unwrap(),
    )
    .unwrap_err();

    assert_eq!(error.code, "validation");
    assert_eq!(error.message, "error.print.rfi.rootEscape");
    assert!(
        selections
            .resolve(
                &selection,
                &caller,
                "files.batch_print",
                "files",
                SelectionKind::File,
            )
            .is_err()
    );
}

#[test]
#[ignore = "set DOCFLOW_VENTOY_RFI_PRINT_FIND_ROOT to a private Ventoy RFI System copy"]
fn operator_ventoy_rfi_print_finder_hands_off_scoped_files_without_source_changes() {
    let root = PathBuf::from(
        std::env::var_os("DOCFLOW_VENTOY_RFI_PRINT_FIND_ROOT")
            .expect("DOCFLOW_VENTOY_RFI_PRINT_FIND_ROOT is required"),
    );
    let number = std::env::var("DOCFLOW_VENTOY_RFI_PRINT_FIND_NUMBER")
        .unwrap_or_else(|_| "CIV-1316".to_string());
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);
    let caller = backend.caller_identity().unwrap();
    let canonical_root = fs::canonicalize(&root).expect("private RFI System copy");
    let root_selection = backend
        .selections
        .issue(
            &caller,
            "files.batch_print",
            "rfi_system_root",
            &root,
            SelectionKind::Folder,
        )
        .unwrap();
    let result = backend
        .find_rfi_print_files(json!({
            "root": root_selection,
            "numbers": [number],
            "firstPage": true,
            "sketch": true,
        }))
        .expect("native finder should discover real Ventoy files");
    let candidates = result["candidates"].as_array().unwrap();
    assert!(
        !candidates.is_empty(),
        "selected private copy must contain the requested RFI"
    );
    let mut source_hashes = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let selection: PathSelection =
            serde_json::from_value(candidate["selection"].clone()).unwrap();
        let path = backend
            .selections
            .resolve(
                &selection,
                &caller,
                "files.batch_print",
                "files",
                SelectionKind::File,
            )
            .unwrap();
        assert_eq!(
            path,
            resolve_scoped_rfi_print_file(
                &backend.selections,
                &selection,
                &caller,
                &canonical_root,
            )
            .unwrap(),
        );
        assert!(path.starts_with(&canonical_root));
        source_hashes.push((path.clone(), crate::fs_ops::file_sha256(&path).unwrap()));
    }
    for (path, hash) in source_hashes {
        assert_eq!(crate::fs_ops::file_sha256(&path).unwrap(), hash);
    }
}
