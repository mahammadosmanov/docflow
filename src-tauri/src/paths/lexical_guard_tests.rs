use super::*;

#[test]
fn rejects_relative_paths() {
    assert!(reject_lexical_escape(Path::new("relative/dir")).is_err());
}

#[test]
fn rejects_parent_directory_components() {
    let escaping = std::env::temp_dir().join("..").join("etc");
    assert!(reject_lexical_escape(&escaping).is_err());
}

#[test]
fn accepts_plain_absolute_paths() {
    // `/root/...` is not absolute on Windows, so build the path from the
    // platform's own absolute temp directory.
    let plain = std::env::temp_dir().join("dir").join("file.pdf");
    assert!(reject_lexical_escape(&plain).is_ok());
}

#[test]
fn expiry_starts_at_the_ttl_boundary() {
    let created = SystemTime::UNIX_EPOCH;
    let ttl = Duration::from_secs(60);
    assert!(!expired(created, created + Duration::from_secs(59), ttl));
    assert!(expired(created, created + ttl, ttl));
}

#[test]
fn a_creation_time_in_the_future_is_not_expired() {
    let now = SystemTime::UNIX_EPOCH;
    assert!(!expired(
        now + Duration::from_secs(5),
        now,
        Duration::from_secs(1)
    ));
}
