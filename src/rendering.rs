use std::collections::HashMap;

use anyhow::Result;
use slint::Image;

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub enum RenderPurpose {
    CurrentSlide,
    NextPreview,
    Thumbnail,
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct RenderRequest {
    pub page_index: u32,
    pub width: i32,
    pub purpose: RenderPurpose,
}

#[derive(Clone)]
pub struct RenderedPage {
    pub image: Image,
    pub aspect_ratio: f32,
}

#[derive(Default)]
pub struct RenderCache {
    pages: HashMap<RenderRequest, RenderedPage>,
}

impl RenderCache {
    pub fn clear(&mut self) {
        self.pages.clear();
    }

    pub fn get_or_render(
        &mut self,
        request: RenderRequest,
        render: impl FnOnce(RenderRequest) -> Result<RenderedPage>,
    ) -> Result<RenderedPage> {
        if let Some(page) = self.pages.get(&request) {
            return Ok(page.clone());
        }

        let page = render(request)?;
        self.pages.insert(request, page.clone());
        Ok(page)
    }

    pub fn retain_presentation_window(
        &mut self,
        current_index: u32,
        total_pages: u32,
        radius: u32,
    ) {
        self.pages.retain(|request, _| {
            should_retain_presentation_request(request, current_index, total_pages, radius)
        });
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.pages.len()
    }
}

fn should_retain_presentation_request(
    request: &RenderRequest,
    current_index: u32,
    total_pages: u32,
    radius: u32,
) -> bool {
    if total_pages == 0 || request.page_index >= total_pages {
        return false;
    }

    match request.purpose {
        RenderPurpose::CurrentSlide => {
            page_is_within_radius(request.page_index, current_index, radius)
        }
        RenderPurpose::NextPreview => request.page_index == current_index.saturating_add(1),
        RenderPurpose::Thumbnail => true,
    }
}

pub fn nearby_page_indices(current_index: u32, total_pages: u32, radius: u32) -> Vec<u32> {
    if total_pages == 0 {
        return Vec::new();
    }

    let start = current_index.saturating_sub(radius);
    let end = current_index.saturating_add(radius).min(total_pages - 1);
    (start..=end).collect()
}

pub fn presentation_preload_order(current_index: u32, total_pages: u32, radius: u32) -> Vec<u32> {
    if total_pages == 0 {
        return Vec::new();
    }

    let mut indices = Vec::new();
    for distance in 1..=radius {
        if let Some(next) = current_index.checked_add(distance) {
            if next < total_pages {
                indices.push(next);
            }
        }
        if current_index >= distance {
            indices.push(current_index - distance);
        }
    }
    indices
}

fn page_is_within_radius(page_index: u32, current_index: u32, radius: u32) -> bool {
    let lower = current_index.saturating_sub(radius);
    let upper = current_index.saturating_add(radius);
    page_index >= lower && page_index <= upper
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::{Rgba8Pixel, SharedPixelBuffer};
    use std::{cell::Cell, rc::Rc};

    fn request(page_index: u32, width: i32, purpose: RenderPurpose) -> RenderRequest {
        RenderRequest {
            page_index,
            width,
            purpose,
        }
    }

    fn rendered_page(aspect_ratio: f32) -> RenderedPage {
        let pixels = [0, 0, 0, 255];
        let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&pixels, 1, 1);
        RenderedPage {
            image: Image::from_rgba8(buffer),
            aspect_ratio,
        }
    }

    #[test]
    fn get_or_render_reuses_matching_cached_page() {
        let calls = Rc::new(Cell::new(0));
        let mut cache = RenderCache::default();
        let request = request(2, 1600, RenderPurpose::CurrentSlide);

        let first = cache
            .get_or_render(request, |_| {
                calls.set(calls.get() + 1);
                Ok(rendered_page(1.5))
            })
            .unwrap();
        let second = cache
            .get_or_render(request, |_| {
                calls.set(calls.get() + 1);
                Ok(rendered_page(1.0))
            })
            .unwrap();

        assert_eq!(calls.get(), 1);
        assert_eq!(first.aspect_ratio, 1.5);
        assert_eq!(second.aspect_ratio, 1.5);
    }

    #[test]
    fn cache_key_separates_width_and_purpose() {
        let calls = Rc::new(Cell::new(0));
        let mut cache = RenderCache::default();

        for request in [
            request(1, 1600, RenderPurpose::CurrentSlide),
            request(1, 600, RenderPurpose::CurrentSlide),
            request(1, 1600, RenderPurpose::NextPreview),
            request(1, 1600, RenderPurpose::Thumbnail),
        ] {
            cache
                .get_or_render(request, |_| {
                    calls.set(calls.get() + 1);
                    Ok(rendered_page(1.0))
                })
                .unwrap();
        }

        assert_eq!(calls.get(), 4);
    }

    #[test]
    fn retain_presentation_window_keeps_current_slide_radius_and_next_preview() {
        let mut cache = RenderCache::default();

        for page_index in 0..7 {
            cache
                .get_or_render(
                    request(page_index, 1600, RenderPurpose::CurrentSlide),
                    |_| Ok(rendered_page(1.0)),
                )
                .unwrap();
            cache
                .get_or_render(request(page_index, 600, RenderPurpose::NextPreview), |_| {
                    Ok(rendered_page(1.0))
                })
                .unwrap();
        }

        cache
            .get_or_render(request(3, 240, RenderPurpose::Thumbnail), |_| {
                Ok(rendered_page(1.0))
            })
            .unwrap();

        cache.retain_presentation_window(3, 7, 2);

        assert_eq!(cache.len(), 7);
        for page_index in 1..=5 {
            assert!(cache.pages.contains_key(&request(
                page_index,
                1600,
                RenderPurpose::CurrentSlide
            )));
        }
        assert!(cache
            .pages
            .contains_key(&request(4, 600, RenderPurpose::NextPreview)));
        assert!(cache
            .pages
            .contains_key(&request(3, 240, RenderPurpose::Thumbnail)));
    }

    #[test]
    fn nearby_page_indices_are_clamped_to_document_bounds() {
        assert_eq!(nearby_page_indices(0, 5, 2), vec![0, 1, 2]);
        assert_eq!(nearby_page_indices(2, 5, 2), vec![0, 1, 2, 3, 4]);
        assert_eq!(nearby_page_indices(4, 5, 2), vec![2, 3, 4]);
        assert!(nearby_page_indices(0, 0, 2).is_empty());
    }

    #[test]
    fn presentation_preload_order_prioritizes_forward_navigation() {
        assert_eq!(presentation_preload_order(3, 8, 2), vec![4, 2, 5, 1]);
        assert_eq!(presentation_preload_order(0, 3, 2), vec![1, 2]);
        assert_eq!(presentation_preload_order(2, 3, 2), vec![1, 0]);
        assert!(presentation_preload_order(0, 0, 2).is_empty());
    }
}
