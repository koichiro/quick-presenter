use std::collections::HashMap;

use anyhow::Result;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

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
    pub estimated_bytes: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct CacheBudget {
    pub max_entries: usize,
    pub max_estimated_bytes: usize,
}

impl Default for CacheBudget {
    fn default() -> Self {
        Self {
            max_entries: 64,
            max_estimated_bytes: 96 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CacheContext {
    pub current_index: u32,
    pub total_pages: u32,
    pub presentation_radius: u32,
}

#[derive(Clone, Debug)]
pub struct RenderedPagePixels {
    pub pixels: SharedPixelBuffer<Rgba8Pixel>,
    pub aspect_ratio: f32,
    pub estimated_bytes: usize,
}

impl From<RenderedPagePixels> for RenderedPage {
    fn from(page: RenderedPagePixels) -> Self {
        Self {
            image: Image::from_rgba8(page.pixels),
            aspect_ratio: page.aspect_ratio,
            estimated_bytes: page.estimated_bytes,
        }
    }
}

#[derive(Default)]
pub struct RenderCache {
    pages: HashMap<RenderRequest, CacheEntry>,
    budget: CacheBudget,
    access_counter: u64,
    estimated_bytes: usize,
}

struct CacheEntry {
    page: RenderedPage,
    last_access: u64,
}

impl RenderCache {
    pub fn with_budget(budget: CacheBudget) -> Self {
        Self {
            budget,
            ..Self::default()
        }
    }

    pub fn clear(&mut self) {
        self.pages.clear();
        self.estimated_bytes = 0;
    }

    pub fn get_or_render(
        &mut self,
        request: RenderRequest,
        render: impl FnOnce(RenderRequest) -> Result<RenderedPage>,
    ) -> Result<RenderedPage> {
        self.access_counter = self.access_counter.wrapping_add(1);
        if let Some(entry) = self.pages.get_mut(&request) {
            entry.last_access = self.access_counter;
            return Ok(entry.page.clone());
        }

        let page = render(request)?;
        self.estimated_bytes = self.estimated_bytes.saturating_add(page.estimated_bytes);
        self.pages.insert(
            request,
            CacheEntry {
                page: page.clone(),
                last_access: self.access_counter,
            },
        );
        self.enforce_budget(None);
        Ok(page)
    }

    pub fn insert(&mut self, request: RenderRequest, page: RenderedPage) {
        self.access_counter = self.access_counter.wrapping_add(1);
        if let Some(entry) = self.pages.insert(
            request,
            CacheEntry {
                page,
                last_access: self.access_counter,
            },
        ) {
            self.estimated_bytes = self
                .estimated_bytes
                .saturating_sub(entry.page.estimated_bytes);
        }

        if let Some(entry) = self.pages.get(&request) {
            self.estimated_bytes = self
                .estimated_bytes
                .saturating_add(entry.page.estimated_bytes);
        }
        self.enforce_budget(None);
    }

    pub fn peek(&self, request: RenderRequest) -> Option<RenderedPage> {
        self.pages.get(&request).map(|entry| entry.page.clone())
    }

    pub fn retain_presentation_window(
        &mut self,
        current_index: u32,
        total_pages: u32,
        radius: u32,
    ) {
        self.retain_requests(|request| {
            should_retain_presentation_request(request, current_index, total_pages, radius)
        });
        self.enforce_budget(Some(CacheContext {
            current_index,
            total_pages,
            presentation_radius: radius,
        }));
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.pages.len()
    }

    fn enforce_budget(&mut self, context: Option<CacheContext>) {
        while self.is_over_budget() {
            let Some(request) = self.eviction_candidate(context) else {
                break;
            };
            self.remove(request);
        }
    }

    fn is_over_budget(&self) -> bool {
        self.pages.len() > self.budget.max_entries
            || self.estimated_bytes > self.budget.max_estimated_bytes
    }

    fn eviction_candidate(&self, context: Option<CacheContext>) -> Option<RenderRequest> {
        self.pages
            .iter()
            .filter(|(request, _)| !is_protected_request(request, context))
            .max_by_key(|(request, entry)| {
                (
                    eviction_priority(request.purpose),
                    u64::MAX.saturating_sub(entry.last_access),
                )
            })
            .map(|(request, _)| *request)
    }

    fn retain_requests(&mut self, keep: impl Fn(&RenderRequest) -> bool) {
        let retained = self
            .pages
            .drain()
            .filter(|(request, _)| keep(request))
            .collect::<HashMap<_, _>>();
        self.estimated_bytes = retained
            .values()
            .map(|entry| entry.page.estimated_bytes)
            .sum();
        self.pages = retained;
    }

    fn remove(&mut self, request: RenderRequest) {
        if let Some(entry) = self.pages.remove(&request) {
            self.estimated_bytes = self
                .estimated_bytes
                .saturating_sub(entry.page.estimated_bytes);
        }
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

pub fn thumbnail_window_indices(current_index: u32, total_pages: u32, radius: u32) -> Vec<u32> {
    nearby_page_indices(current_index, total_pages, radius)
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

pub fn estimated_render_bytes(width: i32, aspect_ratio: f32) -> usize {
    let width = width.max(1) as f32;
    let height = (width / aspect_ratio.max(0.01)).ceil().max(1.0);
    (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4)
}

fn page_is_within_radius(page_index: u32, current_index: u32, radius: u32) -> bool {
    let lower = current_index.saturating_sub(radius);
    let upper = current_index.saturating_add(radius);
    page_index >= lower && page_index <= upper
}

fn is_protected_request(request: &RenderRequest, context: Option<CacheContext>) -> bool {
    let Some(context) = context else {
        return false;
    };

    matches!(request.purpose, RenderPurpose::CurrentSlide)
        && should_retain_presentation_request(
            request,
            context.current_index,
            context.total_pages,
            context.presentation_radius,
        )
}

fn eviction_priority(purpose: RenderPurpose) -> u8 {
    match purpose {
        RenderPurpose::Thumbnail => 3,
        RenderPurpose::NextPreview => 2,
        RenderPurpose::CurrentSlide => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::{Rgba8Pixel, SharedPixelBuffer};
    use std::{cell::Cell, collections::HashSet, rc::Rc};

    fn request(page_index: u32, width: i32, purpose: RenderPurpose) -> RenderRequest {
        RenderRequest {
            page_index,
            width,
            purpose,
        }
    }

    fn rendered_page(aspect_ratio: f32) -> RenderedPage {
        rendered_page_with_bytes(aspect_ratio, 4)
    }

    fn rendered_page_with_bytes(aspect_ratio: f32, estimated_bytes: usize) -> RenderedPage {
        let pixels = [0, 0, 0, 255];
        let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&pixels, 1, 1);
        RenderedPage {
            image: Image::from_rgba8(buffer),
            aspect_ratio,
            estimated_bytes,
        }
    }

    fn loaded_thumbnail_indices(
        cache: &RenderCache,
        total_pages: u32,
        thumbnail_width: i32,
    ) -> HashSet<u32> {
        cache
            .pages
            .keys()
            .filter(|request| {
                request.purpose == RenderPurpose::Thumbnail
                    && request.width == thumbnail_width
                    && request.page_index < total_pages
            })
            .map(|request| request.page_index)
            .collect()
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
    fn cache_eviction_honors_entry_budget() {
        let mut cache = RenderCache::with_budget(CacheBudget {
            max_entries: 3,
            max_estimated_bytes: usize::MAX,
        });

        for page_index in 0..4 {
            cache
                .get_or_render(request(page_index, 240, RenderPurpose::Thumbnail), |_| {
                    Ok(rendered_page(1.0))
                })
                .unwrap();
        }

        assert_eq!(cache.len(), 3);
        assert!(!cache
            .pages
            .contains_key(&request(0, 240, RenderPurpose::Thumbnail)));
    }

    #[test]
    fn cache_eviction_honors_estimated_byte_budget() {
        let mut cache = RenderCache::with_budget(CacheBudget {
            max_entries: usize::MAX,
            max_estimated_bytes: 12,
        });

        for page_index in 0..4 {
            cache
                .get_or_render(request(page_index, 240, RenderPurpose::Thumbnail), |_| {
                    Ok(rendered_page_with_bytes(1.0, 4))
                })
                .unwrap();
        }

        assert_eq!(cache.len(), 3);
        assert_eq!(cache.estimated_bytes, 12);
    }

    #[test]
    fn cache_eviction_removes_thumbnails_before_presentation_pages() {
        let mut cache = RenderCache::with_budget(CacheBudget {
            max_entries: 3,
            max_estimated_bytes: usize::MAX,
        });

        for request in [
            request(0, 1600, RenderPurpose::CurrentSlide),
            request(1, 1600, RenderPurpose::CurrentSlide),
            request(2, 240, RenderPurpose::Thumbnail),
            request(3, 240, RenderPurpose::Thumbnail),
        ] {
            cache
                .get_or_render(request, |_| Ok(rendered_page(1.0)))
                .unwrap();
        }

        assert_eq!(cache.len(), 3);
        assert!(cache
            .pages
            .contains_key(&request(0, 1600, RenderPurpose::CurrentSlide)));
        assert!(cache
            .pages
            .contains_key(&request(1, 1600, RenderPurpose::CurrentSlide)));
        assert_eq!(
            loaded_thumbnail_indices(&cache, 10, 240),
            HashSet::from([3])
        );
    }

    #[test]
    fn protected_presentation_window_survives_budget_enforcement() {
        let mut cache = RenderCache::with_budget(CacheBudget {
            max_entries: 2,
            max_estimated_bytes: usize::MAX,
        });

        for request in [
            request(3, 1600, RenderPurpose::CurrentSlide),
            request(4, 1600, RenderPurpose::CurrentSlide),
            request(7, 240, RenderPurpose::Thumbnail),
        ] {
            cache
                .get_or_render(request, |_| Ok(rendered_page(1.0)))
                .unwrap();
        }

        cache.retain_presentation_window(3, 10, 1);

        assert_eq!(cache.len(), 2);
        assert!(cache
            .pages
            .contains_key(&request(3, 1600, RenderPurpose::CurrentSlide)));
        assert!(cache
            .pages
            .contains_key(&request(4, 1600, RenderPurpose::CurrentSlide)));
    }

    #[test]
    fn nearby_page_indices_are_clamped_to_document_bounds() {
        assert_eq!(nearby_page_indices(0, 5, 2), vec![0, 1, 2]);
        assert_eq!(nearby_page_indices(2, 5, 2), vec![0, 1, 2, 3, 4]);
        assert_eq!(nearby_page_indices(4, 5, 2), vec![2, 3, 4]);
        assert!(nearby_page_indices(0, 0, 2).is_empty());
    }

    #[test]
    fn thumbnail_window_indices_do_not_expand_to_large_document_size() {
        let indices = thumbnail_window_indices(5_000, 10_000, 8);

        assert_eq!(indices.len(), 17);
        assert_eq!(indices.first(), Some(&4_992));
        assert_eq!(indices.last(), Some(&5_008));
    }

    #[test]
    fn presentation_preload_order_prioritizes_forward_navigation() {
        assert_eq!(presentation_preload_order(3, 8, 2), vec![4, 2, 5, 1]);
        assert_eq!(presentation_preload_order(0, 3, 2), vec![1, 2]);
        assert_eq!(presentation_preload_order(2, 3, 2), vec![1, 0]);
        assert!(presentation_preload_order(0, 0, 2).is_empty());
    }
}
