use super::*;
use std::{env, path::PathBuf, time::Instant};

fn is_confirmed_cover(read: &OcrRead) -> bool {
    read.source.starts_with("ocr:fast-cover:confirmed:")
        || read.source.starts_with("ocr:fast-cover:fallback:")
}

#[test]
fn fast_cover_requires_heading_template_mark_and_one_strict_identity() {
    let exact =
        "SUNRISE PROJECT\nREQUEST FOR INSPECTION (RFI)\nRFI No: AZE-SUN-INT-QMS-RFI-CIV-1348";
    assert!(confirmed_cover_read(exact, "test").is_some());
    assert!(
        confirmed_cover_read(
            "INTEC ENERGY SOLUTIONS\nREQUEST FOR INSPECTION\nAZE-SUN-INT-QMS-RFI-CIV-1348",
            "test"
        )
        .is_some()
    );
    assert!(confirmed_cover_read(
        "REQUEST FOR INSPECTION\nSUNRISE PROJECT\nAZE-SUN-INT-QMS-RFI-CIV-1348 AZE-SUN-INT-QMS-RFI-CIV-1349",
        "test"
    )
    .is_none());
    assert!(
        confirmed_cover_read("AZE-SUN-INT-QMS-ITP-CIV-0006 inspection stage", "test").is_none()
    );
    assert!(confirmed_cover_read("REQUEST FOR INSPECTION only", "test").is_none());
    assert!(
        confirmed_cover_read(
            "SUNRISE SOLAR PROJECT INTEC ENERGY SOLUTIONS AZE-SUN-INT-QMS-RFI-CIV-0001",
            "test"
        )
        .is_none()
    );
}

#[test]
fn fast_cover_recognizes_ocr_clv_as_civ_with_alias_evidence() {
    let text = concat!(
        "SUNRISE PROJECT\n",
        "REQUEST FOR INSPECTION (RFI)\n",
        "RFI No: AZE-SUN-INT-QMS-RFI-CLV-1348"
    );
    assert!(exact_codes(text).is_empty());
    assert!(!find(text).ok());

    let read = confirmed_cover_read(text, "test").expect("OCR CLV is an exact alias");
    assert_eq!(read.matched.method, RfiMethod::OcrAlias);
    assert!(
        rfi_matches(text)
            .iter()
            .any(|matched| matched.method == RfiMethod::OcrAlias)
    );
    assert!(
        !cover_signals(text, false).strict_identity,
        "searchable text must not use the OCR-only alias"
    );
    assert_eq!(read.matched.code, "AZE-SUN-INT-QMS-RFI-CIV-1348");
    assert!(read.source.contains("CLV->CIV"), "{}", read.source);
    let alias_match = rfi_matches(text)
        .into_iter()
        .find(|matched| matched.method == RfiMethod::OcrAlias)
        .expect("OCR candidate carries alias provenance");
    let observations = [
        (150, alias_match.clone()),
        (200, alias_match.clone()),
        (250, alias_match),
    ];
    let (matched, votes, dpis) = select_rfi_consensus(&observations, None)
        .expect("three distinct OCR resolutions confirm the alias");
    let consensus = consensus_read(matched, votes, dpis, None, false);
    assert_eq!(consensus.matched.code, "AZE-SUN-INT-QMS-RFI-CIV-1348");
    assert_eq!(consensus.matched.method, RfiMethod::OcrAlias);
    assert!(consensus.source.contains("alias:CLV->CIV"));
    let mixed_identity = rfi_matches("AZE-SUN-INT-QMS-RFI-CLV-1348 AZE-SUN-INT-QMS-RFI-CIV-1348");
    assert_eq!(mixed_identity.len(), 1);
    assert_eq!(mixed_identity[0].method, RfiMethod::Strict);
    for unsupported in ["CLX", "CIL", "CVV", "ARC"] {
        let text = format!(
            "SUNRISE PROJECT\nREQUEST FOR INSPECTION\nAZE-SUN-INT-QMS-RFI-{unsupported}-1348"
        );
        assert!(
            confirmed_cover_read(&text, "test").is_none(),
            "unsupported OCR discipline {unsupported} must not be repaired"
        );
    }
    assert!(
        confirmed_cover_read(
            "SUNRISE PROJECT\nREQUEST FOR INSPECTION\nAZE-SUN-INT-QMS-RFI-CLV-13481",
            "test"
        )
        .is_none(),
        "alias must not match a five-digit suffix"
    );
}

#[test]
#[ignore = "set DOCFLOW_COVER_LAB_PDF and DOCFLOW_COVER_LAB_TESSERACT to check a supplied cover page"]
fn production_fast_cover_detects_the_supplied_first_page() {
    let source = PathBuf::from(
        env::var_os("DOCFLOW_COVER_LAB_PDF").expect("DOCFLOW_COVER_LAB_PDF is required"),
    );
    let tesseract = PathBuf::from(
        env::var_os("DOCFLOW_COVER_LAB_TESSERACT")
            .expect("DOCFLOW_COVER_LAB_TESSERACT is required"),
    );
    let ocr = NativeOcr::with_tesseract(tesseract);
    let started = Instant::now();
    let read = read_fast_cover_page(&ocr, &source, 0)
        .expect("fast detector should process the supplied cover");
    eprintln!(
        "supplied_cover elapsed_ms={} confirmed={} source={}",
        started.elapsed().as_millis(),
        is_confirmed_cover(&read),
        read.source
    );
    assert!(
        is_confirmed_cover(&read),
        "first page must be confirmed cover"
    );
}

#[test]
#[ignore = "set DOCFLOW_BAKED_180_COVER_PDF and DOCFLOW_COVER_LAB_TESSERACT to check the pixel-rotated CIV-1158 cover"]
fn production_fast_cover_scans_past_wordy_direction_zero_noise() {
    let source = PathBuf::from(
        env::var_os("DOCFLOW_BAKED_180_COVER_PDF")
            .expect("DOCFLOW_BAKED_180_COVER_PDF is required"),
    );
    let tesseract = PathBuf::from(
        env::var_os("DOCFLOW_COVER_LAB_TESSERACT")
            .expect("DOCFLOW_COVER_LAB_TESSERACT is required"),
    );
    let ocr = NativeOcr::with_tesseract(tesseract);
    let read = read_fast_cover_page(&ocr, &source, 0).expect("fast OCR should process the cover");
    assert!(is_confirmed_cover(&read), "{}", read.source);
    assert_eq!(read.matched.code, "AZE-SUN-INT-QMS-RFI-CIV-1158");
}

#[test]
#[ignore = "set DOCFLOW_COVER_LAB_PDF and DOCFLOW_COVER_LAB_TESSERACT to benchmark a supplied scan"]
fn benchmark_fast_cover_scan_against_current_page_ocr() {
    let source = PathBuf::from(
        env::var_os("DOCFLOW_COVER_LAB_PDF").expect("DOCFLOW_COVER_LAB_PDF is required"),
    );
    let tesseract = PathBuf::from(
        env::var_os("DOCFLOW_COVER_LAB_TESSERACT")
            .expect("DOCFLOW_COVER_LAB_TESSERACT is required"),
    );
    assert!(tesseract.is_file(), "Tesseract executable is unavailable");
    let ocr = NativeOcr::with_tesseract(tesseract);
    let page_count = crate::pdf::inspect(&source)
        .expect("sample PDF should be inspectable")
        .page_count;
    let current_started = Instant::now();
    let mut current_covers = 0;
    for page_index in 0..page_count {
        let started = Instant::now();
        let read = ocr
            .read_page(&source, page_index)
            .expect("current OCR should process the sample page");
        current_covers +=
            usize::from(read.matched.method == RfiMethod::Strict && read.matched.confidence >= 0.8);
        eprintln!(
            "current page={} elapsed_ms={} exact_identity={}",
            page_index + 1,
            started.elapsed().as_millis(),
            read.matched.method == RfiMethod::Strict && read.matched.confidence >= 0.8
        );
    }
    let current_elapsed = current_started.elapsed();

    let fast_started = Instant::now();
    let mut fast_covers = 0;
    for page_index in 0..page_count {
        let started = Instant::now();
        let read = read_fast_cover_page(&ocr, &source, page_index)
            .expect("fast OCR should process the sample page");
        let confirmed = is_confirmed_cover(&read);
        fast_covers += usize::from(confirmed);
        eprintln!(
            "fast page={} elapsed_ms={} confirmed={confirmed} source={}",
            page_index + 1,
            started.elapsed().as_millis(),
            read.source
        );
    }
    let fast_elapsed = fast_started.elapsed();
    eprintln!(
        "pages={page_count} current_ms={} current_exact_pages={current_covers} fast_ms={} fast_cover_candidates={fast_covers}",
        current_elapsed.as_millis(),
        fast_elapsed.as_millis()
    );
    assert!(
        page_count > 0,
        "sample PDF should contain at least one page"
    );
    assert_eq!(fast_covers, 1, "sample must have one cover");
    assert_eq!(current_covers, 1, "current OCR must recognize its cover");
}

#[test]
#[ignore = "set DOCFLOW_COVER_LAB_MANIFEST and DOCFLOW_COVER_LAB_TESSERACT to test a local PDF corpus"]
fn benchmark_production_fast_cover_scan_corpus_and_rotations() {
    let manifest = PathBuf::from(
        env::var_os("DOCFLOW_COVER_LAB_MANIFEST").expect("DOCFLOW_COVER_LAB_MANIFEST is required"),
    );
    let tesseract = PathBuf::from(
        env::var_os("DOCFLOW_COVER_LAB_TESSERACT")
            .expect("DOCFLOW_COVER_LAB_TESSERACT is required"),
    );
    assert!(tesseract.is_file(), "Tesseract executable is unavailable");
    let sources = fs::read_to_string(&manifest)
        .expect("cover corpus manifest should be readable")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    assert!(!sources.is_empty(), "cover corpus should not be empty");

    let ocr = NativeOcr::with_tesseract(tesseract);
    let temporary = tempfile::tempdir().expect("temporary cover corpus should open");
    let mut source_hits = 0;
    let mut continuation_false_covers = 0;
    let mut continuation_uncertain = 0;
    let mut rotation_hits = 0;
    let mut rotation_misses = 0;
    let mut rotation_variants = 0;

    for (sample_index, source) in sources.iter().enumerate() {
        assert!(source.is_file(), "cover corpus PDF is unavailable");
        let facts = crate::pdf::inspect(source).expect("cover corpus PDF should be valid");
        assert!(facts.page_count > 0, "cover corpus PDF should not be empty");
        let source_label = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("sample.pdf");
        let source_result = scan_corpus_pages(&ocr, source, 0..facts.page_count);
        source_hits += usize::from(source_result.0);
        continuation_false_covers += source_result.1;
        continuation_uncertain += source_result.2;
        eprintln!(
            "corpus file={source_label} pages={} cover={} continuation_false_covers={} continuation_uncertain={}",
            facts.page_count, source_result.0, source_result.1, source_result.2
        );

        for angle in [90_i64, 180, 270] {
            let rotated = temporary
                .path()
                .join(format!("sample-{sample_index}-{angle}.pdf"));
            let rotations = facts
                .pages
                .iter()
                .enumerate()
                .map(|(page_index, page)| (page_index, (page.rotation + angle).rem_euclid(360)))
                .collect::<Vec<_>>();
            if let Err(error) = crate::pdf::rotate_pages_incremental(source, &rotated, &rotations) {
                eprintln!("corpus_rotation=SKIP file={source_label} angle={angle} reason={error}");
                continue;
            }
            rotation_variants += 1;
            let rotated_facts =
                crate::pdf::inspect(&rotated).expect("rotated test copy should remain inspectable");
            let cover_pages = [0]
                .into_iter()
                .chain((facts.page_count >= 20).then_some(facts.page_count - 1))
                .collect::<Vec<_>>();
            let result = scan_corpus_pages(&ocr, &rotated, cover_pages.iter().copied());
            rotation_hits += usize::from(result.0);
            rotation_misses += usize::from(!result.0);
            continuation_false_covers += result.1;
            continuation_uncertain += result.2;
            assert_eq!(rotated_facts.page_count, facts.page_count);
        }
    }

    eprintln!(
        "corpus_summary sources={} original_cover_hits={source_hits} continuation_false_covers={continuation_false_covers} continuation_uncertain={continuation_uncertain} rotated_variants={rotation_variants} rotated_cover_hits={rotation_hits} rotated_cover_misses={rotation_misses}",
        sources.len()
    );
    assert_eq!(
        source_hits,
        sources.len(),
        "every source cover must be detected"
    );
    assert_eq!(
        continuation_false_covers, 0,
        "continuations must not split groups"
    );
    assert_eq!(
        continuation_uncertain, 0,
        "continuations must not enter Review"
    );
    assert_eq!(
        rotation_hits, rotation_variants,
        "rotated covers must be detected"
    );
    assert_eq!(rotation_misses, 0, "rotated covers must not be missed");
}

fn scan_corpus_pages<I>(ocr: &NativeOcr, source: &Path, pages: I) -> (bool, usize, usize)
where
    I: IntoIterator<Item = usize>,
{
    let mut first_page_cover = false;
    let mut continuation_false_covers = 0;
    let mut continuation_uncertain = 0;
    for page_index in pages {
        let read = read_fast_cover_page(ocr, source, page_index)
            .expect("corpus page should render and scan");
        let confirmed = is_confirmed_cover(&read);
        if page_index == 0 {
            first_page_cover = confirmed;
        } else {
            continuation_false_covers += usize::from(confirmed);
            continuation_uncertain +=
                usize::from(!confirmed && read.source.starts_with("ocr:fast-cover:uncertain:"));
        }
    }
    (
        first_page_cover,
        continuation_false_covers,
        continuation_uncertain,
    )
}
