//! Diagnostic-only App Group socket experiment. Never enables Store presentation control.
#[cfg(target_os = "macos")]
#[path = "store_ipc_probe/runtime.rs"]
mod runtime;

fn main() {
    #[cfg(target_os = "macos")]
    if let Err(error) = runtime::run() {
        eprintln!(
            "{}",
            serde_json::json!({"probe": 1, "error": error.to_string()})
        );
        std::process::exit(7);
    }
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("Store IPC probe requires macOS 14.4 or later.");
        std::process::exit(7);
    }
}
