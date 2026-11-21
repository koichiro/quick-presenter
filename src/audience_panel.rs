//! Window geometry boundary for the Audience Live disclosure.
use slint::winit_030::{winit, WinitWindowAccessor};
use slint::{ComponentHandle, PhysicalSize};

use crate::PresenterWindow;

#[derive(Default)]
pub struct AudiencePanelGeometry {
    sizes: Option<PanelSizes>,
    last_requested: Option<PhysicalSize>,
    pending_from: Option<PhysicalSize>,
}

struct PanelSizes {
    expanded: PhysicalSize,
    collapsed: PhysicalSize,
    scale: f32,
}

impl AudiencePanelGeometry {
    pub fn observe_size(&mut self, current: PhysicalSize) {
        if self.last_requested == Some(current) {
            self.pending_from = None;
        } else if self.pending_from != Some(current) {
            self.sizes = None;
            self.pending_from = None;
        }
    }

    fn cached_target(
        &mut self,
        current: PhysicalSize,
        scale: f32,
        expanded: bool,
    ) -> Option<PhysicalSize> {
        self.observe_size(current);
        self.sizes
            .as_ref()
            .filter(|sizes| sizes.scale == scale)
            .map(|sizes| {
                if expanded {
                    sizes.expanded
                } else {
                    sizes.collapsed
                }
            })
    }

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
            self.last_requested = None;
            self.pending_from = None;
            return;
        }
        // A native resize may still be pending when disclosure changes again.
        let target = if let Some(target) = self.cached_target(current, scale, expanded) {
            target
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
        let requested = PhysicalSize::new(target.width, limit_height(target.height));
        self.pending_from = (current != requested).then_some(current);
        self.last_requested = Some(requested);
        window.set_size(requested);
        #[cfg(target_os = "windows")]
        crate::windows_window::fit_presenter_to_work_area(presenter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending_collapse() -> AudiencePanelGeometry {
        AudiencePanelGeometry {
            sizes: Some(PanelSizes {
                expanded: PhysicalSize::new(800, 900),
                collapsed: PhysicalSize::new(800, 600),
                scale: 1.0,
            }),
            last_requested: Some(PhysicalSize::new(800, 600)),
            pending_from: Some(PhysicalSize::new(800, 900)),
        }
    }

    #[test]
    fn expand_before_collapse_ack_restores_original_size() {
        let mut geometry = pending_collapse();
        assert_eq!(
            geometry.cached_target(PhysicalSize::new(800, 900), 1.0, true),
            Some(PhysicalSize::new(800, 900))
        );
    }

    #[test]
    fn manual_resize_to_other_cached_size_after_ack_invalidates_pair() {
        let mut geometry = pending_collapse();
        geometry.observe_size(PhysicalSize::new(800, 600));
        assert!(geometry
            .cached_target(PhysicalSize::new(800, 900), 1.0, true)
            .is_none());
    }
}
