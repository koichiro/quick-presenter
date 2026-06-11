fn main() {
    slint_build::compile("ui/app.slint").unwrap();

    println!("cargo:rerun-if-changed=assets/icons/windows/quick-presenter.ico");

    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_windows_icon();
    }
}

fn embed_windows_icon() {
    winresource::WindowsResource::new()
        .set_icon("assets/icons/windows/quick-presenter.ico")
        .compile()
        .expect("failed to embed Windows application icon");
}
