use std::{
    collections::VecDeque,
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Instant,
};

use docflow_lib::{ocr::NativeOcr, operations::process_scans::OcrAdapter, rfi_code::RfiMethod};
use serde::Serialize;

#[derive(Serialize)]
struct ResultRow {
    filename: String,
    code: String,
    method: String,
    confidence: f64,
    source: String,
    error: Option<String>,
    milliseconds: u128,
}

fn method_name(method: RfiMethod) -> &'static str {
    match method {
        RfiMethod::Strict => "strict",
        RfiMethod::Repaired => "repaired",
        RfiMethod::OcrAlias => "ocr_alias",
        RfiMethod::Fuzzy => "fuzzy",
        RfiMethod::Miss => "miss",
    }
}

fn pdfs(directory: &Path) -> Vec<PathBuf> {
    let mut paths = fs::read_dir(directory)
        .expect("read corpus directory")
        .map(|entry| entry.expect("read corpus entry").path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn main() {
    let arguments = env::args_os().collect::<Vec<_>>();
    assert_eq!(
        arguments.len(),
        4,
        "usage: ocr_corpus_eval CORPUS TESSERACT OUTPUT"
    );
    let corpus = PathBuf::from(&arguments[1]);
    let tesseract = PathBuf::from(&arguments[2]);
    let output = PathBuf::from(&arguments[3]);
    let paths = pdfs(&corpus);
    assert!(!paths.is_empty(), "corpus is empty");

    let workers = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .min(4)
        .min(paths.len());
    let total_started = Instant::now();
    let queue = Arc::new(Mutex::new(VecDeque::from(paths)));
    let adapter = NativeOcr::with_tesseract(tesseract);
    let (sender, receiver) = mpsc::channel();

    thread::scope(|scope| {
        for _ in 0..workers {
            let queue = Arc::clone(&queue);
            let adapter = adapter.clone();
            let sender = sender.clone();
            scope.spawn(move || {
                loop {
                    let path = queue.lock().expect("lock corpus queue").pop_front();
                    let Some(path) = path else { break };
                    let started = Instant::now();
                    let filename = path
                        .file_name()
                        .expect("PDF has a filename")
                        .to_string_lossy()
                        .into_owned();
                    let row = match adapter.read(&path) {
                        Ok(read) => ResultRow {
                            filename,
                            code: read.matched.code,
                            method: method_name(read.matched.method).to_string(),
                            confidence: read.matched.confidence,
                            source: read.source,
                            error: None,
                            milliseconds: started.elapsed().as_millis(),
                        },
                        Err(error) => ResultRow {
                            filename,
                            code: String::new(),
                            method: "error".to_string(),
                            confidence: 0.0,
                            source: String::new(),
                            error: Some(format!("{error:?}")),
                            milliseconds: started.elapsed().as_millis(),
                        },
                    };
                    sender.send(row).expect("send OCR result");
                }
            });
        }
        drop(sender);
    });

    let mut rows = receiver.into_iter().collect::<Vec<_>>();
    rows.sort_by(|left, right| left.filename.cmp(&right.filename));
    let payload = serde_json::json!({
        "corpus": corpus,
        "fileCount": rows.len(),
        "workers": workers,
        "totalMilliseconds": total_started.elapsed().as_millis(),
        "results": rows,
    });
    let mut output_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .expect("create OCR result JSON; output must not already exist");
    output_file
        .write_all(&serde_json::to_vec_pretty(&payload).expect("serialize results"))
        .expect("write OCR result JSON");
    println!(
        "files={} workers={} elapsed_ms={}",
        rows.len(),
        workers,
        total_started.elapsed().as_millis()
    );
}
