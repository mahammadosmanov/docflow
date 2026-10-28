use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Read, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    cap_fs::ArtifactParentCreateOutcome,
    contracts::{Plan, Step},
    errors::BackendError,
    paths::ValidatedGrantRootRoster,
    plan_store::{ClaimedS5Apply, PlanBinding},
    progress::GlobalMutationPermit,
};

const MAX_JOURNAL_BYTES: u64 = 8 * 1024 * 1024;
const MAX_JOURNAL_STEPS: usize = 100_000;
const MAX_CLEANUP_REVIEW_EVIDENCE_PER_JOB: usize = 16;
pub(crate) const MAX_RETAINED_ARTIFACTS_PER_JOURNAL: usize = 8;
pub(crate) const MAX_RECOVERY_LOCATOR_SCALARS: usize = 512;
pub(crate) const MAX_S5_RETAINED_FILES: usize = 256;
pub(crate) const MAX_S5_RETAINED_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub(crate) const MAX_JOURNAL_ROOT_ENTRIES_SCANNED: usize = 4096;
pub(crate) const MAX_PENDING_S5_RECORDS: usize = 256;
pub(crate) const MAX_INVALID_S5_NOTICES: usize = 64;
pub(crate) const MAX_INVALID_S5_NOTICE_BYTES: u64 = 4096;
pub(crate) const MAX_RECOVERY_ENTRIES: usize = 256;

mod fs_io;
mod model;
mod permits;
pub use model::*;
pub(crate) use permits::*;
mod lock;
mod recovery;
mod s5;
mod s5_accessors;
mod s5_session;
mod store;
use fs_io::*;
use lock::*;
use recovery::*;
pub use s5::*;
use s5_session::*;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod s5_tests;
