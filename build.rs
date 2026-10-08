fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build_xpc_bridge();
    }
    let slint_source =
        if std::env::var_os("CARGO_CFG_TARGET_OS").as_deref() == Some("linux".as_ref()) {
            linux_slint_source()
        } else {
            std::path::PathBuf::from("ui/app.slint")
        };
    slint_build::compile(slint_source).unwrap();

    println!("cargo:rerun-if-changed=assets/icons/windows/quick-presenter.ico");
    println!("cargo:rerun-if-changed=ui/app.slint");

    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_windows_icon();
    }
}

fn build_xpc_bridge() {
    use std::{path::PathBuf, process::Command};
    println!("cargo:rerun-if-changed=src/macos_renderer_xpc.c");
    println!("cargo:rerun-if-changed=src/macos_file_access.c");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        _ => panic!("unsupported macOS renderer architecture"),
    };
    let mut compiler = Command::new("xcrun");
    compiler.args([
        "clang",
        "-arch",
        arch,
        "-fblocks",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-c",
        "src/macos_renderer_xpc.c",
    ]);
    if std::env::var("PROFILE").as_deref() == Ok("release") {
        compiler.arg("-DNDEBUG");
    }
    assert!(compiler
        .arg("-o")
        .arg(out.join("renderer-xpc.o"))
        .status()
        .unwrap()
        .success());
    assert!(Command::new("xcrun")
        .args([
            "clang",
            "-arch",
            arch,
            "-Wall",
            "-Wextra",
            "-Werror",
            "-c",
            "src/macos_file_access.c",
            "-o"
        ])
        .arg(out.join("file-access.o"))
        .status()
        .unwrap()
        .success());
    assert!(Command::new("xcrun")
        .args(["libtool", "-static", "-o"])
        .arg(out.join("librenderer_xpc.a"))
        .arg(out.join("renderer-xpc.o"))
        .arg(out.join("file-access.o"))
        .status()
        .unwrap()
        .success());
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=renderer_xpc");
    println!("cargo:rustc-link-lib=framework=Security");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");
}

fn linux_slint_source() -> std::path::PathBuf {
    const BEGIN_MARKER: &str = "    // BEGIN_NATIVE_MENU_BAR\n";
    const END_MARKER: &str = "    // END_NATIVE_MENU_BAR\n";

    let manifest_dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let source_path = manifest_dir.join("ui/app.slint");
    let mut source = std::fs::read_to_string(&source_path).expect("failed to read Slint source");
    let begin = source
        .find(BEGIN_MARKER)
        .expect("missing native menu begin marker");
    let end = source
        .find(END_MARKER)
        .expect("missing native menu end marker")
        + END_MARKER.len();
    source.replace_range(begin..end, "");

    let asset_prefix = manifest_dir
        .join("assets")
        .to_string_lossy()
        .replace('\\', "/");
    source = source.replace(
        "@image-url(\"../assets/",
        &format!("@image-url(\"{asset_prefix}/"),
    );

    let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let generated_path = out_dir.join("app-linux.slint");
    std::fs::write(&generated_path, source).expect("failed to write Linux Slint source");
    generated_path
}

fn embed_windows_icon() {
    winresource::WindowsResource::new()
        .set_icon("assets/icons/windows/quick-presenter.ico")
        .compile()
        .expect("failed to embed Windows application icon");
}
