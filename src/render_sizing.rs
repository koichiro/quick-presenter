//! Physical slide-surface sizing, independent of windowing and PDFium.
use crate::{render_controller::CURRENT_RENDER_WIDTH, rendering::CacheBudget};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderSizingPolicy {
    current_width: i32,
}

impl Default for RenderSizingPolicy {
    fn default() -> Self {
        Self {
            current_width: CURRENT_RENDER_WIDTH,
        }
    }
}

impl RenderSizingPolicy {
    pub fn current_width(self) -> i32 {
        self.current_width
    }
    pub fn for_surface_width(width: u32) -> Self {
        let bounded = width.clamp(1600, 2560);
        Self {
            current_width: if bounded == 1600 {
                1600
            } else {
                (bounded.div_ceil(128) * 128).min(2560) as i32
            },
        }
    }

    pub fn cache_budget(self) -> CacheBudget {
        let bytes = |w: usize| 4 * w * (3 * w).div_ceil(4);
        let working = 5 * bytes(self.current_width as usize) + bytes(600) + 17 * bytes(180);
        CacheBudget {
            max_entries: 64,
            max_estimated_bytes: (working * 3)
                .div_ceil(2)
                .clamp(96 * 1024 * 1024, 192 * 1024 * 1024),
        }
    }

    // Discover geometry at baseline first. A rejected upgrade must never make an
    // otherwise renderable tall PDF unusable or repeat a failing request.
    // Pixel-derived aspect ratios include rounding; reserve two height pixels
    // below the helper ceiling before requesting a larger bitmap.
    pub fn page_width(self, aspect: Option<f32>) -> i32 {
        match aspect {
            Some(aspect)
                if crate::renderer_limits::render_dimensions(aspect, 1.0, self.current_width)
                    .is_ok()
                    && self.current_width as f32 / aspect
                        <= (crate::renderer_limits::MAX_DIMENSION - 2) as f32 =>
            {
                self.current_width
            }
            _ => CURRENT_RENDER_WIDTH,
        }
    }
}

#[derive(Default)]
pub struct SizingSamples {
    candidate: Option<RenderSizingPolicy>,
}

impl SizingSamples {
    pub fn observe(
        &mut self,
        width: Option<u32>,
        current: RenderSizingPolicy,
    ) -> Option<RenderSizingPolicy> {
        let candidate = width
            .filter(|width| *width > 0)
            .map(RenderSizingPolicy::for_surface_width);
        let stable = candidate.is_some() && candidate == self.candidate;
        self.candidate = candidate;
        candidate.filter(|candidate| stable && *candidate != current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn buckets_floor_clamp_and_budget_are_bounded() {
        for (surface, expected) in [
            (1024, 1600),
            (1600, 1600),
            (1601, 1664),
            (1921, 2048),
            (2047, 2048),
            (2049, 2176),
            (3840, 2560),
            (u32::MAX, 2560),
        ] {
            assert_eq!(
                RenderSizingPolicy::for_surface_width(surface).current_width,
                expected
            );
        }
        let mut previous = 0;
        for surface in 1..8192 {
            let budget = RenderSizingPolicy::for_surface_width(surface).cache_budget();
            assert_eq!(budget.max_entries, 64);
            assert!((96 * 1024 * 1024..=192 * 1024 * 1024).contains(&budget.max_estimated_bytes));
            assert!(budget.max_estimated_bytes >= previous);
            previous = budget.max_estimated_bytes;
        }
        assert_eq!(
            RenderSizingPolicy::default()
                .cache_budget()
                .max_estimated_bytes,
            100_663_296
        );
        assert_eq!(
            RenderSizingPolicy::for_surface_width(2560)
                .cache_budget()
                .max_estimated_bytes,
            151_554_600
        );
    }
    #[test]
    fn unstable_hidden_and_zero_samples_do_not_change_policy() {
        let mut samples = SizingSamples::default();
        let current = RenderSizingPolicy::default();
        for width in [
            Some(2048),
            None,
            Some(2048),
            Some(0),
            Some(2048),
            Some(2049),
            Some(2048),
        ] {
            assert!(samples.observe(width, current).is_none());
        }
        assert_eq!(
            samples.observe(Some(2048), current).unwrap().current_width,
            2048
        );
    }
    #[test]
    fn unknown_and_tall_geometry_use_baseline() {
        let policy = RenderSizingPolicy::for_surface_width(2560);
        assert_eq!(policy.page_width(None), 1600);
        assert_eq!(policy.page_width(Some(0.5)), 1600);
        assert_eq!(policy.page_width(Some(f32::NAN)), 1600);
        assert_eq!(policy.page_width(Some(4.0 / 3.0)), 2560);
        assert_eq!(policy.page_width(Some(16.0 / 9.0)), 2560);
    }
}
