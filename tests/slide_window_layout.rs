#[test]
fn slide_window_does_not_cap_fullscreen_size_to_its_windowed_size() {
    let source = include_str!("../ui/app.slint");
    let slide_window = source
        .split_once("export component SlideWindow inherits Window {")
        .expect("missing SlideWindow component")
        .1;

    assert!(!slide_window.contains("max-width: root.slide-window-width;"));
    assert!(!slide_window.contains("max-height: root.slide-window-height;"));
}
