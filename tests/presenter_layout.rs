use std::rc::Rc;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::ComponentHandle;

slint::include_modules!();

struct LayoutPlatform(Rc<MinimalSoftwareWindow>);

impl Platform for LayoutPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

// A software window exercises the compiled Slint layout without AppKit's
// main-thread or fullscreen requirements, including sizes larger than the display.
#[test]
fn presenter_panes_resize_without_losing_notes_or_overlapping_the_footer() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(LayoutPlatform(window.clone()))).unwrap();
    let presenter = PresenterWindow::new().unwrap();
    presenter.set_audience_address("2001:db8:1234:5678:abcd:ef01:2345:6789".into());
    presenter.set_audience_count(12345);
    presenter.set_has_notes(true);
    presenter.set_notes_text(
        "Speaker notes remain readable across window sizes.\n次のポイントを説明します。".into(),
    );
    presenter.global::<NotesTypography>().on_paragraphs(|text| {
        Rc::new(slint::VecModel::from(
            text.split('\n')
                .map(slint::SharedString::from)
                .collect::<Vec<_>>(),
        ))
        .into()
    });
    presenter.show().unwrap();

    for native_menu in [true, false] {
        presenter.set_use_native_menu_bar(native_menu);
        for aspect in [16.0 / 9.0, 4.0 / 3.0, 0.6] {
            presenter.set_current_page_aspect_ratio(aspect);
            presenter.set_next_page_aspect_ratio(aspect);
            for (width, height) in [
                (800, 842),
                (1024, 1002),
                (1280, 900),
                (1920, 1080),
                (2560, 1440),
                (3840, 2160),
                (5120, 2880),
                (1200, 1240),
            ] {
                let mut collapsed_main_bottom = 0.0;
                for expanded in [false, true] {
                    presenter.set_audience_panel_expanded(expanded);
                    window.set_size(slint::PhysicalSize::new(width, height));
                    let mut pixels = vec![slint::Rgb8Pixel::default(); (width * height) as usize];
                    window.request_redraw();
                    assert!(window.draw_if_needed(|renderer| {
                        renderer.render(&mut pixels, width as usize);
                    }));

                    let current = presenter.get_presenter_current_column_width();
                    let next = presenter.get_presenter_next_column_width();
                    let thumbnails = presenter.get_presenter_thumbnail_width();
                    let context =
                    format!("{width}x{height}, aspect={aspect}, native_menu={native_menu}, expanded={expanded}");
                    assert!(current >= next, "{context}");
                    assert!(next >= 260.0, "{context}");
                    assert!((128.0..=192.0).contains(&thumbnails), "{context}");
                    assert!(
                        (presenter.get_notes_layout_outer_width() - current).abs() <= 1.0,
                        "{context}"
                    );
                    assert!(presenter.get_notes_visible_height() >= 180.0, "{context}");
                    assert!(presenter.get_notes_layout_slide_width() > 0.0, "{context}");
                    assert!(
                        presenter.get_presenter_next_preview_height() > 0.0,
                        "{context}"
                    );
                    assert!(
                        presenter.get_presenter_next_preview_height() <= next / aspect + 1.0,
                        "{context}"
                    );
                    assert!(
                        presenter.get_audience_panel_right() + 8.0
                            <= presenter.get_thumbnails_left(),
                        "{context}"
                    );
                    assert!(
                        (presenter.get_thumbnails_bottom()
                            - presenter.get_audience_footer_bottom())
                        .abs()
                            <= 1.0,
                        "{context}"
                    );
                    assert!(
                        presenter.get_audience_toggle_y() >= presenter.get_notes_area_bottom(),
                        "{context}"
                    );
                    assert!(
                        presenter.get_audience_url_bottom() <= height as f32 - 36.0,
                        "{context}"
                    );
                    assert!(
                        (presenter.get_notes_layout_slide_width()
                            - presenter.get_presenter_next_slide_width() * 2.0)
                            .abs()
                            <= 1.0,
                        "Rendered slide widths must stay at 2:1: {context}"
                    );
                    assert_eq!(
                        presenter.get_audience_history_visible(),
                        expanded,
                        "{context}"
                    );
                    assert_eq!(presenter.get_audience_url_visible(), expanded, "{context}");
                    if expanded {
                        assert!(
                            collapsed_main_bottom > presenter.get_notes_area_bottom() + 200.0,
                            "Collapsing must reclaim space: {context}"
                        );
                    } else {
                        collapsed_main_bottom = presenter.get_notes_area_bottom();
                        assert_eq!(presenter.get_audience_panel_height(), 48.0, "{context}");
                    }
                    assert!(presenter.get_audience_toolbar_ordered(), "{context}");
                    assert!(
                        presenter.get_audience_address_controls_inline(),
                        "{context}"
                    );
                    assert_eq!(presenter.get_audience_count_label(), "👥(12345)");
                    assert!(
                        (presenter.get_audience_reactions_width()
                            - presenter.get_audience_panel_width())
                        .abs()
                            <= 1.0,
                        "History should fill the Audience Live pane: {context}"
                    );
                    if next > 260.0 {
                        assert!((current - next * 2.0).abs() <= 1.0, "{context}");
                    }
                    if let Some(directory) = std::env::var_os("QP_PRESENTER_LAYOUT_SNAPSHOTS") {
                        std::fs::create_dir_all(&directory).unwrap();
                        let mut bytes = format!("P6\n{width} {height}\n255\n").into_bytes();
                        for pixel in pixels {
                            bytes.extend_from_slice(&[pixel.r, pixel.g, pixel.b]);
                        }
                        std::fs::write(
                            std::path::PathBuf::from(directory).join(format!(
                                "{width}x{height}-{aspect:.2}-{native_menu}-{expanded}.ppm"
                            )),
                            bytes,
                        )
                        .unwrap();
                    }
                }
            }
        }
    }
}
