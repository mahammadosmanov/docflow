use super::*;

#[test]
fn pdf_publish_retries_transient_access_errors_without_overwriting() {
    let fixture = tempfile::TempDir::new().unwrap();
    let temporary = fixture.path().join(".review.pdf.docflow-test.tmp");
    let destination = fixture.path().join("review.pdf");
    fs::write(&temporary, b"completed PDF bytes").unwrap();
    let mut calls = 0;

    publish_pdf_no_clobber_with(
        &temporary,
        &destination,
        4,
        |source, target| {
            calls += 1;
            if calls < 3 {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "simulated OneDrive lock",
                ))
            } else {
                fs::hard_link(source, target)
            }
        },
        |_| {},
    )
    .unwrap();

    assert_eq!(calls, 3);
    assert_eq!(fs::read(destination).unwrap(), b"completed PDF bytes");
    assert!(!temporary.exists());
}

#[test]
fn pdf_publish_falls_back_to_exclusive_copy_after_persistent_access_errors() {
    let fixture = tempfile::TempDir::new().unwrap();
    let temporary = fixture.path().join(".review.pdf.docflow-test.tmp");
    let destination = fixture.path().join("review.pdf");
    fs::write(&temporary, b"completed PDF bytes").unwrap();
    let mut calls = 0;

    publish_pdf_no_clobber_with(
        &temporary,
        &destination,
        3,
        |_, _| {
            calls += 1;
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "simulated persistent OneDrive lock",
            ))
        },
        |_| {},
    )
    .unwrap();

    assert_eq!(calls, 3);
    assert_eq!(fs::read(destination).unwrap(), b"completed PDF bytes");
    assert!(!temporary.exists());
}

#[test]
fn pdf_publish_never_overwrites_an_existing_destination() {
    let fixture = tempfile::TempDir::new().unwrap();
    let temporary = fixture.path().join(".review.pdf.docflow-test.tmp");
    let destination = fixture.path().join("review.pdf");
    fs::write(&temporary, b"new PDF bytes").unwrap();
    fs::write(&destination, b"existing user bytes").unwrap();

    let error = publish_pdf_no_clobber_with(
        &temporary,
        &destination,
        3,
        |source, target| fs::hard_link(source, target),
        |_| {},
    )
    .unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(destination).unwrap(), b"existing user bytes");
    assert_eq!(fs::read(temporary).unwrap(), b"new PDF bytes");
}
