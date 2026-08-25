use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::errors::BackendError;

#[derive(Clone, Debug, Default)]
pub struct GlobalMutationLock {
    held: Arc<AtomicBool>,
}

impl GlobalMutationLock {
    pub fn try_acquire(&self) -> Result<GlobalMutationPermit, BackendError> {
        self.held
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| GlobalMutationPermit {
                held: Arc::clone(&self.held),
            })
            .map_err(|_| BackendError::operation_in_progress())
    }
}

#[derive(Debug)]
pub struct GlobalMutationPermit {
    held: Arc<AtomicBool>,
}

impl Drop for GlobalMutationPermit {
    fn drop(&mut self) {
        self.held.store(false, Ordering::Release);
    }
}

#[derive(Clone, Debug)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn checkpoint(&self) -> Result<(), BackendError> {
        if self.cancelled.load(Ordering::Acquire) {
            Err(BackendError::cancelled())
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
pub struct CancellationRegistry {
    requests: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl CancellationRegistry {
    pub fn register(&self, request_id: &str) -> CancellationToken {
        let cancelled = lock_requests(&self.requests)
            .entry(request_id.to_string())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone();
        CancellationToken { cancelled }
    }

    pub fn cancel(&self, request_id: &str) -> bool {
        let cancelled = lock_requests(&self.requests).get(request_id).cloned();
        if let Some(cancelled) = cancelled {
            cancelled.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }

    pub fn finish(&self, request_id: &str) {
        lock_requests(&self.requests).remove(request_id);
    }

    pub fn revoke_all(&self) {
        let mut requests = lock_requests(&self.requests);
        for cancelled in requests.values() {
            cancelled.store(true, Ordering::Release);
        }
        requests.clear();
    }
}

fn lock_requests(
    requests: &Mutex<HashMap<String, Arc<AtomicBool>>>,
) -> MutexGuard<'_, HashMap<String, Arc<AtomicBool>>> {
    requests
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_cancellation_is_observable_at_stage_and_pre_effect_checkpoints() {
        let registry = CancellationRegistry::default();
        let stage_token = registry.register("request-1");
        let pre_effect_token = stage_token.clone();

        assert!(stage_token.checkpoint().is_ok());
        assert!(registry.cancel("request-1"));
        assert_eq!(
            stage_token
                .checkpoint()
                .expect_err("stage checkpoint should observe cancellation")
                .code,
            "operation_cancelled"
        );
        assert_eq!(
            pre_effect_token
                .checkpoint()
                .expect_err("pre-effect checkpoint should observe cancellation")
                .code,
            "operation_cancelled"
        );

        registry.finish("request-1");
        assert!(!registry.cancel("request-1"));
        assert!(!registry.cancel("unknown-request"));
    }

    #[test]
    fn global_mutation_lock_rejects_concurrent_apply_and_releases_on_drop() {
        let first_lock = GlobalMutationLock::default();
        let second_lock = first_lock.clone();
        let permit = first_lock
            .try_acquire()
            .expect("first mutation should acquire the global lock");

        let error = second_lock
            .try_acquire()
            .expect_err("concurrent mutation must be rejected");
        assert_eq!(error.code, "operation_in_progress");

        drop(permit);
        assert!(second_lock.try_acquire().is_ok());
    }
}
