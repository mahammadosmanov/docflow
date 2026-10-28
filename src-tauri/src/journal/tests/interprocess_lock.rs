use super::*;

#[test]
fn interprocess_lock_waits_briefly_for_a_release_on_another_thread() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    let store = JournalStore::new(root);
    let (locked_tx, locked_rx) = std::sync::mpsc::channel::<()>();

    std::thread::scope(|scope| {
        let holder = scope.spawn(move || {
            let permit = JournalStore::new(root).try_acquire_mutation_lock().unwrap();
            locked_tx.send(()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(30));
            drop(permit);
        });
        locked_rx.recv().unwrap();
        let waited = store.try_acquire_mutation_lock();
        assert!(
            waited.is_ok(),
            "a short hold must be waited out, not reported as contention"
        );
        drop(waited);
        holder.join().unwrap();
    });
}

#[test]
fn interprocess_lock_reenters_on_holding_thread_and_excludes_other_threads() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    let contended_elsewhere = || {
        std::thread::scope(|scope| {
            scope
                .spawn(|| JournalStore::new(root).try_acquire_mutation_lock().is_err())
                .join()
                .unwrap()
        })
    };
    let store = JournalStore::new(root);

    let outer = store.try_acquire_mutation_lock().unwrap();
    let nested = store
        .try_acquire_mutation_lock()
        .expect("the holding thread must re-enter its own lock");
    assert!(contended_elsewhere(), "another thread must stay excluded");

    drop(nested);
    assert!(contended_elsewhere(), "outer permit still excludes others");

    drop(outer);
    let free_elsewhere = std::thread::scope(|scope| {
        scope
            .spawn(|| JournalStore::new(root).try_acquire_mutation_lock().is_ok())
            .join()
            .unwrap()
    });
    assert!(
        free_elsewhere,
        "lock must release after the last permit drops"
    );
}
