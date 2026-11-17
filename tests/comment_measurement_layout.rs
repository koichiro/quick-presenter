use std::rc::Rc;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::ComponentHandle;

slint::include_modules!();

struct MeasurementPlatform;

impl Platform for MeasurementPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer))
    }
}

#[test]
fn persistent_presenter_measures_new_comments_after_audience_hide_and_show() {
    slint::platform::set_platform(Box::new(MeasurementPlatform)).unwrap();
    let presenter = PresenterWindow::new().unwrap();
    let audience = AudienceWindow::new().unwrap();
    presenter.show().unwrap();
    audience.show().unwrap();

    presenter.set_comment_measure_text("Hello 世界".into());
    let short_width = presenter.get_comment_measure_width();
    assert!(short_width > 100.0, "32px comment width: {short_width}");
    presenter.set_comment_measure_text("Hello 世界 Hello 世界".into());
    let long_width = presenter.get_comment_measure_width();
    assert!(long_width > short_width * 1.8);

    for _ in 0..3 {
        audience.hide().unwrap();
        audience.show().unwrap();
        presenter.set_comment_measure_text("Hello 世界".into());
        assert_eq!(presenter.get_comment_measure_width(), short_width);
        presenter.set_comment_measure_text("Hello 世界 Hello 世界".into());
        assert_eq!(presenter.get_comment_measure_width(), long_width);
    }
}
