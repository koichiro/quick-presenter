pub const DEFAULT_SLIDE_ASPECT_RATIO: f32 = 16.0 / 9.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FittedSlideSurface {
    pub width: f32,
    pub height: f32,
}

pub fn sanitize_aspect_ratio(aspect_ratio: f32) -> f32 {
    if aspect_ratio.is_finite() && aspect_ratio > 0.0 {
        aspect_ratio
    } else {
        DEFAULT_SLIDE_ASPECT_RATIO
    }
}

pub fn fit_slide_surface(
    parent_width: f32,
    parent_height: f32,
    aspect_ratio: f32,
) -> FittedSlideSurface {
    if parent_width <= 0.0 || parent_height <= 0.0 {
        return FittedSlideSurface {
            width: 0.0,
            height: 0.0,
        };
    }

    let aspect_ratio = sanitize_aspect_ratio(aspect_ratio);
    let parent_aspect_ratio = parent_width / parent_height;

    if parent_aspect_ratio > aspect_ratio {
        FittedSlideSurface {
            width: parent_height * aspect_ratio,
            height: parent_height,
        }
    } else {
        FittedSlideSurface {
            width: parent_width,
            height: parent_width / aspect_ratio,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_slide_surface_uses_full_width_when_parent_is_taller_than_slide() {
        let surface = fit_slide_surface(1024.0, 720.0, 16.0 / 9.0);

        assert_eq!(surface.width, 1024.0);
        assert_eq!(surface.height, 576.0);
    }

    #[test]
    fn fit_slide_surface_uses_full_height_when_parent_is_wider_than_slide() {
        let surface = fit_slide_surface(1024.0, 720.0, 4.0 / 3.0);

        assert_eq!(surface.width, 960.0);
        assert_eq!(surface.height, 720.0);
    }

    #[test]
    fn fit_slide_surface_fills_parent_when_aspects_match() {
        let surface = fit_slide_surface(1280.0, 720.0, 16.0 / 9.0);

        assert_eq!(surface.width, 1280.0);
        assert_eq!(surface.height, 720.0);
    }

    #[test]
    fn fit_slide_surface_falls_back_for_invalid_aspect_ratio() {
        let surface = fit_slide_surface(1024.0, 720.0, 0.0);

        assert_eq!(surface.width, 1024.0);
        assert_eq!(surface.height, 576.0);
    }

    #[test]
    fn fit_slide_surface_returns_empty_surface_for_empty_parent() {
        let surface = fit_slide_surface(0.0, 720.0, 16.0 / 9.0);

        assert_eq!(
            surface,
            FittedSlideSurface {
                width: 0.0,
                height: 0.0
            }
        );
    }
}
