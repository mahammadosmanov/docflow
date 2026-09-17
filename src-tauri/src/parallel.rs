use std::thread;

use rayon::prelude::*;

const MAX_ANALYSIS_WORKERS: usize = 4;

pub(crate) fn worker_count(item_count: usize) -> usize {
    thread::available_parallelism()
        .map_or(1, usize::from)
        .saturating_sub(1)
        .clamp(1, MAX_ANALYSIS_WORKERS)
        .min(item_count.max(1))
}

pub(crate) fn bounded_map<T, R, F>(items: &[T], map: F) -> Result<(usize, Vec<R>), String>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Send + Sync,
{
    let workers = worker_count(items.len());
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

pub(crate) fn bounded_map_stream<T, R, F, C>(
    items: &[T],
    map: F,
    mut completed: C,
) -> Result<(usize, Vec<R>), String>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Send + Sync,
    C: FnMut(usize, &R) -> Result<(), String>,
{
    let workers = worker_count(items.len());
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .thread_name(|index| format!("docflow-analysis-{index}"))
        .build()
        .map_err(|error| format!("analysis worker pool could not start: {error}"))?;
    let (sender, receiver) = std::sync::mpsc::channel::<(usize, R)>();
    let mut ordered = std::iter::repeat_with(|| None)
        .take(items.len())
        .collect::<Vec<_>>();
    thread::scope(|scope| -> Result<(), String> {
        let worker = scope.spawn(move || {
            pool.install(|| {
                items.par_iter().enumerate().for_each(|(index, item)| {
                    let _ = sender.send((index, map(item)));
                });
            });
        });
        for (index, result) in receiver {
            completed(index, &result)?;
            ordered[index] = Some(result);
        }
        worker
            .join()
            .map_err(|_| "analysis worker pool panicked".to_string())?;
        Ok(())
    })?;
    let results = ordered
        .into_iter()
        .map(|result| result.ok_or_else(|| "analysis worker did not return a result".to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((workers, results))
}
