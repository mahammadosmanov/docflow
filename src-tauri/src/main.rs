#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

fn main() {
    if let Some(status) = docflow_lib::smoke::maybe_run(std::env::args()) {
        std::process::exit(status);
    }
    docflow_lib::run();
}
