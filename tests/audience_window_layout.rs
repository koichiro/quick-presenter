#[test]
fn audience_window_does_not_bind_maximum_size_to_windowed_dimensions() {
    let source = include_str!("../ui/app.slint");
    let audience_window = source
        .split_once("export component AudienceWindow inherits Window {")
        .expect("missing AudienceWindow component")
        .1;

    assert!(!audience_window.contains("max-width: root.audience-window-width;"));
    assert!(!audience_window.contains("max-height: root.audience-window-height;"));
}
