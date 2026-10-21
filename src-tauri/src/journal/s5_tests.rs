use super::*;

#[test]
fn interprocess_mutation_lock_contends_across_store_instances_and_releases_on_drop() {
    let fixture = tempfile::tempdir().unwrap();
    let first_store = JournalStore::new(fixture.path());
    let second_store = JournalStore::new(fixture.path());

    let first = first_store.try_acquire_mutation_lock().unwrap();
    let error = second_store.try_acquire_mutation_lock().unwrap_err();
    assert_eq!(error.code, "operation_in_progress");

    drop(first);
    second_store.try_acquire_mutation_lock().unwrap();
}
