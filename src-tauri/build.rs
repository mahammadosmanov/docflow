use std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::Path,
};

fn assert_final_runtime(manifest_dir: &Path) {
    let root = manifest_dir
        .parent()
        .expect("src-tauri must have a repository parent");
    for relative in [
        "docflow-sidecar.spec",
        "pyproject.toml",
        "requirements.txt",
        "requirements-dev.txt",
        "src/docflow",
        "tools/build_sidecar.py",
        "tools/ci_ocr_smoke.py",
        "tools/version_manager.py",
    ] {
        let path = root.join(relative);
        println!("cargo:rerun-if-changed={}", path.display());
        assert!(
            !path.exists(),
            "final DocFlow builds reject removed runtime path: {relative}"
        );
    }
    for (relative, forbidden) in [
        ("src-tauri/Cargo.toml", "tauri-plugin-shell"),
        ("src-tauri/tauri.conf.json", "externalBin"),
        ("src-tauri/tauri.conf.json", "docflow-sidecar"),
        ("src-tauri/src/backend.rs", "struct FallbackSupervisor"),
        ("src-tauri/src/lib.rs", "tauri_plugin_shell"),
    ] {
        let path = root.join(relative);
        println!("cargo:rerun-if-changed={}", path.display());
        let source = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("final runtime guard could not read {relative}: {error}")
        });
        assert!(
            !source.contains(forbidden),
            "final DocFlow builds reject {forbidden} in {relative}"
        );
    }
}

fn hash_ocr_sources(root: &Path, relative: &Path, hash: &mut DefaultHasher) {
    let path = root.join(relative);
    println!("cargo:rerun-if-changed={}", path.display());
    if path.is_dir() {
        let mut children = fs::read_dir(&path)
            .expect("read OCR source directory")
            .map(|entry| entry.expect("read OCR source entry").file_name())
            .collect::<Vec<_>>();
        children.sort_unstable();
        for child in children {
            let name = child.to_string_lossy();
            if name != "tests.rs" && !name.ends_with("_tests.rs") {
                hash_ocr_sources(root, &relative.join(child), hash);
            }
        }
        return;
    }
    relative.hash(hash);
    fs::read(path).expect("read OCR profile input").hash(hash);
}

fn ocr_revision(root: &Path) -> u64 {
    let mut hash = DefaultHasher::new();
    for input in [
        "src/ocr.rs",
        "src/ocr",
        "src/pdf.rs",
        "src/pdf",
        "src/windows_pdf.rs",
        "src/rfi_code.rs",
        "src/ocr_cache.rs",
        "native/pdfkit.m",
    ] {
        hash_ocr_sources(root, Path::new(input), &mut hash);
    }
    let lock_path = root.join("Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock_path.display());
    let lock = fs::read_to_string(lock_path).expect("read OCR dependency lock");
    let version = format!(
        "name = \"docflow\"\nversion = \"{}\"",
        std::env::var("CARGO_PKG_VERSION").expect("Cargo supplies package version")
    );
    lock.replace(&version, "name = \"docflow\"").hash(&mut hash);
    for input in ["DOCFLOW_TESSERACT_VERSION", "DOCFLOW_TESSERACT_SHA256"] {
        println!("cargo:rerun-if-env-changed={input}");
        std::env::var_os(input).hash(&mut hash);
    }
    hash.finish()
}

fn main() {
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .expect("Cargo supplies CARGO_MANIFEST_DIR");
    assert_final_runtime(&manifest_dir);
    println!(
        "cargo:rustc-env=DOCFLOW_OCR_REVISION={:016x}",
        ocr_revision(&manifest_dir)
    );
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("native/pdfkit.m")
            .flag("-fobjc-arc")
            .compile("docflow_pdfkit");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rustc-link-lib=framework=PDFKit");
        println!("cargo:rerun-if-changed=native/pdfkit.m");
    }
    tauri_build::build()
}
