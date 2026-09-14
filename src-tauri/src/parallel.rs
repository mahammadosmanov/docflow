use std::thread;

use rayon::prelude::*;

const MAX_ANALYSIS_WORKERS: usize = 4;

pub(crate) fn bounded_map<T, R, F>(items: &[T], map: F) -> Result<(usize, Vec<R>), String>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Send + Sync,
{
    let workers = thread::available_parallelism()
        .map_or(1, usize::from)
        .saturating_sub(1)
        .clamp(1, MAX_ANALYSIS_WORKERS)
        .min(items.len().max(1));
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .thread_name(|index| format!("docflow-analysis-{index}"))
        .build()
        .map_err(|error| format!("analysis worker pool could not start: {error}"))?;
    Ok((
        workers,
        pool.install(|| items.par_iter().map(map).collect()),
    ))
}
