use std::{fs, path::Path};

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

fn main() {
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .expect("Cargo supplies CARGO_MANIFEST_DIR");
    assert_final_runtime(&manifest_dir);
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
