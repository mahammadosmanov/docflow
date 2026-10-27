//! Retained-artifact storage accounting used by the S5 storage budget.

use super::*;

const S5_RETAINED_PREFIXES: [&str; 4] = [
    ".docflow-s5-backup-",
    ".docflow-s5-private-output-",
    ".docflow-s5-retained-original-",
    ".docflow-s5-retained-applied-",
];

pub(crate) struct RetainedStorageUsage {
    pub(crate) file_count: usize,
    pub(crate) logical_charge_bytes: u64,
}

pub(crate) struct RetainedStorageReservation {
    pub(crate) file_count: usize,
    pub(crate) logical_charge_bytes: u64,
}

impl RetainedStorageReservation {
    pub(crate) fn for_apply(
        original_default_stream_bytes: u64,
        original_auxiliary_charge_bytes: u64,
    ) -> io::Result<Self> {
        let logical_charge_bytes = original_default_stream_bytes
            .checked_mul(2)
            .and_then(|value| value.checked_add(crate::ooxml::MAX_PACKAGE_BYTES))
            .and_then(|value| {
                original_auxiliary_charge_bytes
                    .checked_mul(3)
                    .and_then(|auxiliary| value.checked_add(auxiliary))
            })
            .ok_or_else(|| io::Error::other("retained storage reservation overflowed"))?;
        Ok(Self {
            file_count: 3,
            logical_charge_bytes,
        })
    }

    pub(crate) fn for_undo(
        current_applied_default_stream_bytes: u64,
        current_applied_auxiliary_charge_bytes: u64,
    ) -> io::Result<Self> {
        Ok(Self {
            file_count: 1,
            logical_charge_bytes: current_applied_default_stream_bytes
                .checked_add(current_applied_auxiliary_charge_bytes)
                .ok_or_else(|| io::Error::other("retained storage reservation overflowed"))?,
        })
    }
}

impl AuthorizedPath {
    pub(crate) fn s5_retained_storage_usage(
        &self,
        max_entries: usize,
    ) -> io::Result<RetainedStorageUsage> {
        let directory = self.open_directory_no_follow()?;
        let mut usage = RetainedStorageUsage {
            file_count: 0,
            logical_charge_bytes: 0,
        };
        for name in directory.entry_names()? {
            let Some(name_text) = name.to_str() else {
                continue;
            };
            if !S5_RETAINED_PREFIXES
                .iter()
                .any(|prefix| name_text.starts_with(prefix))
            {
                continue;
            }
            usage.file_count = usage
                .file_count
                .checked_add(1)
                .ok_or_else(|| io::Error::other("retained artifact count overflowed"))?;
            if usage.file_count > max_entries {
                return Err(io::Error::other(
                    "retained artifact count exceeded the limit",
                ));
            }
            let file = directory.open_file_no_follow(&name, false)?;
            let metadata = file.metadata()?;
            if !metadata.is_file() {
                return Err(io::Error::other("retained artifact is not a regular file"));
            }
            let snapshot = FileSecuritySnapshot::capture(&file, None)?;
            let charge = metadata
                .len()
                .checked_add(snapshot.auxiliary_charge_bytes())
                .ok_or_else(|| io::Error::other("retained artifact charge overflowed"))?;
            usage.logical_charge_bytes = usage
                .logical_charge_bytes
                .checked_add(charge)
                .ok_or_else(|| io::Error::other("retained storage usage overflowed"))?;
        }
        Ok(usage)
    }
}
