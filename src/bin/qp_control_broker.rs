#[cfg(target_os = "macos")]
fn main() {
    std::process::exit(quick_presenter::control::store_xpc::broker_main());
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("The Store control broker is available only on macOS.");
    std::process::exit(2);
}
