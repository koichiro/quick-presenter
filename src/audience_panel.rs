//! Window geometry boundary for the Audience Live disclosure.
use slint::winit_030::{winit, WinitWindowAccessor};
use slint::{ComponentHandle, PhysicalSize};

use crate::PresenterWindow;

#[derive(Default)]
pub struct AudiencePanelGeometry {
    sizes: Option<PanelSizes>,
}

struct PanelSizes {
    expanded: PhysicalSize,
    collapsed: PhysicalSize,
    scale: f32,
}

impl AudiencePanelGeometry {
    pub fn set_expanded(&mut self, presenter: &PresenterWindow, expanded: bool) {
        if presenter.get_audience_panel_expanded() == expanded {
            return;
        }
        #[cfg(target_os = "windows")]
        crate::windows_window::fit_presenter_to_work_area(presenter);
        let window = presenter.window();
        let current = window.size();
        let scale = window.scale_factor();
        let hidden_height =
            ((presenter.get_audience_expanded_height() - 48.0) * scale).round() as u32;
        presenter.set_audience_panel_expanded(expanded);
        let limit_height = |height: u32| {
            let available = presenter.get_presenter_work_area_height();
            if available > 0.0 {
                let minimum = (presenter.get_presenter_minimum_height() * scale).ceil() as u32;
                height
                    .max(minimum)
                    .min(((available * scale).floor() as u32).max(minimum))
            } else {
                height
            }
        };

        // Let the OS retain ownership of maximized/fullscreen geometry.
        if window.is_maximized() || window.is_fullscreen() {
            self.sizes = None;
            return;
        }
        // Retain the exact device-pixel pair until the presenter resizes the
        // window or changes display scaling. This prevents accumulated rounding
        // drift and also handles another toggle before native resize completion.
        let target = if let Some(sizes) = self.sizes.as_ref().filter(|sizes| {
            sizes.scale == scale && (current == sizes.expanded || current == sizes.collapsed)
        }) {
            if expanded {
                sizes.expanded
            } else {
                sizes.collapsed
            }
        } else {
            let target = PhysicalSize::new(
                current.width,
                limit_height(if expanded {
                    current.height.saturating_add(hidden_height)
                } else {
                    current.height.saturating_sub(hidden_height).max(1)
                }),
            );
            self.sizes = Some(PanelSizes {
                expanded: if expanded { target } else { current },
                collapsed: if expanded { current } else { target },
                scale,
            });
            target
        };
        // Apply the new minimum before requesting a smaller native window.
        // Slint's deferred property pass can still hold the expanded minimum.
        let _ = window.with_winit_window(|native| {
            native.set_min_inner_size(Some(winit::dpi::LogicalSize::new(
                800.0,
                f64::from(presenter.get_presenter_minimum_height()),
            )));
        });
        window.set_size(PhysicalSize::new(target.width, limit_height(target.height)));
        #[cfg(target_os = "windows")]
        crate::windows_window::fit_presenter_to_work_area(presenter);
    }
}
