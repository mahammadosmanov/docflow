pub mod auth;
mod backend;
mod cap_fs;
pub mod clipboard;
pub mod cloud;
mod concrete_document_sources;
mod concrete_documents;
pub mod contracts;
pub mod credentials;
pub mod diagnostics;
pub mod email_extract;
pub mod errors;
pub mod fs_ops;
pub mod gemini;
pub mod http;
pub mod journal;
mod leaf_transaction;
pub mod naming;
mod native;
pub mod ocr;
pub mod ocr_cache;
pub mod ooxml;
pub mod operations;
pub mod outlook_reply_draft;
pub mod page_range;
mod parallel;
mod path_selection;
pub mod paths;
pub mod pdf;
mod pdf_publish;
pub mod plan_store;
pub mod platform;
pub mod print_queue;
pub mod progress;
pub mod registry;
pub mod review;
pub mod rfi_code;
pub mod rfi_follow_up;
pub mod rfi_log;
pub mod rfi_system_sync;
pub mod scan_index;
pub mod settings;
pub mod smoke;
pub mod undo;
pub mod watch;
#[cfg(target_os = "windows")]
mod windows_pdf;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let router = backend::BackendRouter::new(app.handle().clone())
                .map_err(|error| std::io::Error::other(error.message))?;
            router.start_watch();
            app.manage(router);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![backend::frontend_call])
        .run(tauri::generate_context!())
        .expect("error while running DocFlow");
}
