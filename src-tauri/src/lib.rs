pub mod auth;
mod backend;
mod cap_fs;
pub mod clipboard;
pub mod cloud;
pub mod contracts;
pub mod credentials;
pub mod email_extract;
pub mod errors;
pub mod fs_ops;
pub mod gemini;
pub mod http;
pub mod journal;
pub mod naming;
mod native;
pub mod ocr;
pub mod ooxml;
pub mod operations;
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
pub mod rfi_log;
pub mod settings;
pub mod smoke;
pub mod undo;
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
            app.manage(router);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![backend::frontend_call])
        .run(tauri::generate_context!())
        .expect("error while running DocFlow");
}
