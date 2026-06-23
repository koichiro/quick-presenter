use std::rc::Rc;
use std::time::Instant;

use slint::{ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel, Weak};

use crate::{
    app_state::{AppState, ThumbnailState},
    clock::current_clock_label,
    errors::PresenterMessage,
    presentation::PageSnapshot,
    render_controller::{CURRENT_RENDER_WIDTH, PREVIEW_RENDER_WIDTH, THUMBNAIL_RENDER_WIDTH},
    rendering::{RenderCache, RenderPurpose, RenderRequest, RenderedPage},
    window_controller::AppWindowRefs,
    PresenterWindow, ThumbnailItem,
};

pub fn apply_opening_state_to_windows(windows: &AppWindowRefs, title: &str) {
    let placeholder = placeholder_slide();

    if let Some(presenter) = windows.presenter.upgrade() {
        presenter.set_document_title(title.into());
        presenter.set_page_label("".into());
        presenter.set_current_page_image(placeholder.image.clone());
        presenter.set_current_page_aspect_ratio(placeholder.aspect_ratio);
        presenter.set_has_next_page(false);
        presenter.set_next_page_image(placeholder.image.clone());
        presenter.set_next_page_aspect_ratio(placeholder.aspect_ratio);
        presenter.set_status_text("Opening PDF...".into());
        presenter.set_current_page_index(0);
        presenter.set_thumbnails(ModelRc::new(Rc::new(VecModel::from(
            Vec::<ThumbnailItem>::new(),
        ))));
        presenter.set_has_notes(false);
        presenter.set_notes_text("".into());
    }

    if let Some(slide) = windows.slide.upgrade() {
        slide.set_page_aspect_ratio(placeholder.aspect_ratio);
        slide.set_page_image(placeholder.image);
    }
}

pub fn apply_snapshot_to_windows(
    windows: &AppWindowRefs,
    state: &AppState,
    snapshot: &PageSnapshot,
) {
    let current_request = current_slide_request(snapshot.current_index);
    let cached_current = state.render_cache.peek(current_request);
    let presenter_current = cached_current.clone().unwrap_or_else(placeholder_slide);
    let audience_current = audience_current_slide(
        cached_current,
        state.audience_slide.last_good_current.clone(),
    );
    let next = snapshot.next_index.and_then(|page_index| {
        state.render_cache.peek(RenderRequest {
            page_index,
            width: PREVIEW_RENDER_WIDTH,
            purpose: RenderPurpose::NextPreview,
        })
    });
    let next_placeholder = placeholder_slide();

    if let Some(presenter) = windows.presenter.upgrade() {
        presenter.set_current_page_image(presenter_current.image.clone());
        presenter.set_current_page_aspect_ratio(presenter_current.aspect_ratio);
        presenter.set_has_next_page(snapshot.next_index.is_some());
        if let Some(next) = next.as_ref() {
            presenter.set_next_page_image(next.image.clone());
            presenter.set_next_page_aspect_ratio(next.aspect_ratio);
        } else {
            presenter.set_next_page_image(next_placeholder.image.clone());
            presenter.set_next_page_aspect_ratio(next_placeholder.aspect_ratio);
        }
        presenter.set_document_title(snapshot.title.clone().into());
        presenter.set_page_label(snapshot.page_label.clone().into());
        presenter.set_clock_time_label(current_clock_label().into());
        presenter.set_elapsed_time_label(state.timer.elapsed_label_at(Instant::now()).into());
        presenter.set_status_text(presenter_status_text_for_snapshot(state, snapshot).into());
        presenter.set_current_page_index(thumbnail_current_row_index(
            snapshot.total_pages,
            snapshot.current_index,
            crate::THUMBNAIL_CACHE_RADIUS,
        ));
        presenter.set_thumbnails(thumbnail_model(
            &state.thumbnails,
            &state.render_cache,
            snapshot.current_index,
        ));

        let current_note = state.notes.note_for_page_index(snapshot.current_index);
        presenter.set_has_notes(current_note.is_some());
        presenter.set_notes_text(current_note.unwrap_or_default().into());
    }

    if let Some(slide) = windows.slide.upgrade() {
        slide.set_page_aspect_ratio(audience_current.aspect_ratio);
        slide.set_page_image(if state.black_screen.is_active() {
            black_slide_image()
        } else {
            audience_current.image
        });
    }
}

pub fn set_presenter_message(weak: &Weak<PresenterWindow>, message: PresenterMessage) {
    if let Some(app) = weak.upgrade() {
        app.set_status_text(message.text().into());
    }
}

pub fn presenter_status_text(state: &AppState) -> String {
    if state.black_screen.is_active() {
        "Black screen active. Audience slide is hidden.".to_owned()
    } else {
        state.status_text.clone()
    }
}

fn presenter_status_text_for_snapshot(state: &AppState, snapshot: &PageSnapshot) -> String {
    if state.black_screen.is_active() {
        return "Black screen active. Audience slide is hidden.".to_owned();
    }

    if state.audience_slide.failed_current_page == Some(snapshot.current_index) {
        return state.status_text.clone();
    }

    if state
        .render_cache
        .peek(current_slide_request(snapshot.current_index))
        .is_none()
    {
        return "Rendering page...".to_owned();
    }

    state.status_text.clone()
}

fn audience_current_slide(
    cached_current: Option<RenderedPage>,
    last_good_current: Option<RenderedPage>,
) -> RenderedPage {
    cached_current
        .or(last_good_current)
        .unwrap_or_else(placeholder_slide)
}

fn current_slide_request(page_index: u32) -> RenderRequest {
    RenderRequest {
        page_index,
        width: CURRENT_RENDER_WIDTH,
        purpose: RenderPurpose::CurrentSlide,
    }
}

pub fn thumbnail_model(
    thumbnails: &ThumbnailState,
    cache: &RenderCache,
    current_index: u32,
) -> ModelRc<ThumbnailItem> {
    let placeholder = thumbnail_placeholder_image();
    let items = (0..thumbnails.total_pages)
        .map(|index| {
            let image = cache
                .peek(RenderRequest {
                    page_index: index,
                    width: THUMBNAIL_RENDER_WIDTH,
                    purpose: RenderPurpose::Thumbnail,
                })
                .map(|thumbnail| thumbnail.image)
                .unwrap_or_else(|| placeholder.clone());
            ThumbnailItem {
                page_index: i32::try_from(index).unwrap_or(i32::MAX),
                page_label: format!("{}", index + 1).into(),
                image,
                is_current: index == current_index,
            }
        })
        .collect::<Vec<_>>();

    ModelRc::new(Rc::new(VecModel::from(items)))
}

pub fn thumbnail_current_row_index(total_pages: u32, current_index: u32, radius: u32) -> i32 {
    let _ = radius;
    if total_pages == 0 {
        return 0;
    }
    i32::try_from(current_index.min(total_pages.saturating_sub(1))).unwrap_or(i32::MAX)
}

pub fn placeholder_slide() -> RenderedPage {
    const WIDTH: u32 = 16;
    const HEIGHT: u32 = 9;

    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let edge = x == 0 || y == 0 || x == WIDTH - 1 || y == HEIGHT - 1;
            let value = if edge { 82 } else { 31 };
            pixels.extend_from_slice(&[value, value, value, 255]);
        }
    }
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&pixels, WIDTH, HEIGHT);
    RenderedPage {
        image: slint::Image::from_rgba8(buffer),
        aspect_ratio: 16.0 / 9.0,
        estimated_bytes: (WIDTH * HEIGHT * 4) as usize,
    }
}

fn thumbnail_placeholder_image() -> slint::Image {
    const WIDTH: u32 = 16;
    const HEIGHT: u32 = 9;

    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let edge = x == 0 || y == 0 || x == WIDTH - 1 || y == HEIGHT - 1;
            let value = if edge { 82 } else { 31 };
            pixels.extend_from_slice(&[value, value, value, 255]);
        }
    }
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&pixels, WIDTH, HEIGHT);
    slint::Image::from_rgba8(buffer)
}

pub fn black_slide_image() -> slint::Image {
    const WIDTH: u32 = 16;
    const HEIGHT: u32 = 9;

    let mut pixels = vec![0; (WIDTH * HEIGHT * 4) as usize];
    for alpha in pixels.iter_mut().skip(3).step_by(4) {
        *alpha = 255;
    }
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&pixels, WIDTH, HEIGHT);
    slint::Image::from_rgba8(buffer)
}

pub fn recent_file_menu_labels(labels: Vec<String>) -> ModelRc<SharedString> {
    ModelRc::new(Rc::new(VecModel::from(
        labels
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::PresentationState;
    use slint::Model;

    fn rendered_page(aspect_ratio: f32) -> RenderedPage {
        RenderedPage {
            image: placeholder_slide().image,
            aspect_ratio,
            estimated_bytes: 64,
        }
    }

    fn thumbnail_items(total_pages: u32, current_index: u32) -> Vec<ThumbnailItem> {
        let model = thumbnail_model(
            &ThumbnailState { total_pages },
            &RenderCache::default(),
            current_index,
        );

        (0..model.row_count())
            .map(|row| model.row_data(row).expect("thumbnail row should exist"))
            .collect()
    }

    #[test]
    fn audience_current_slide_prefers_cached_current_page() {
        let selected = audience_current_slide(
            Some(rendered_page(4.0 / 3.0)),
            Some(rendered_page(16.0 / 9.0)),
        );

        assert_eq!(selected.aspect_ratio, 4.0 / 3.0);
    }

    #[test]
    fn audience_current_slide_keeps_last_good_when_current_cache_misses() {
        let selected = audience_current_slide(None, Some(rendered_page(16.0 / 9.0)));

        assert_eq!(selected.aspect_ratio, 16.0 / 9.0);
    }

    #[test]
    fn presenter_status_reports_rendering_for_current_page_cache_miss() {
        let state = AppState {
            presentation: PresentationState::open_document("Deck", 2),
            status_text: "Ready".to_owned(),
            ..AppState::default()
        };
        let snapshot = state.presentation.snapshot().expect("presentation is open");

        assert_eq!(
            presenter_status_text_for_snapshot(&state, &snapshot),
            "Rendering page..."
        );
    }

    #[test]
    fn presenter_status_keeps_current_page_render_failure_message() {
        let mut state = AppState {
            presentation: PresentationState::open_document("Deck", 2),
            status_text: "Could not render this page. Try another PDF or page.".to_owned(),
            ..AppState::default()
        };
        let snapshot = state.presentation.snapshot().expect("presentation is open");
        state.audience_slide.failed_current_page = Some(snapshot.current_index);

        assert_eq!(
            presenter_status_text_for_snapshot(&state, &snapshot),
            "Could not render this page. Try another PDF or page."
        );
    }

    #[test]
    fn thumbnail_model_includes_all_pages_for_large_documents() {
        let items = thumbnail_items(10_000, 5_000);

        assert_eq!(items.len(), 10_000);
        assert_eq!(items.first().unwrap().page_index, 0);
        assert_eq!(items.last().unwrap().page_index, 9_999);
    }

    #[test]
    fn thumbnail_model_keeps_absolute_rows_at_document_edges() {
        let first_page_items = thumbnail_items(10, 0);
        let last_page_items = thumbnail_items(10, 9);

        assert_eq!(
            first_page_items
                .iter()
                .map(|item| item.page_index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
        );
        assert_eq!(
            last_page_items
                .iter()
                .map(|item| item.page_index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
        );
    }

    #[test]
    fn thumbnail_model_marks_only_current_page() {
        let items = thumbnail_items(10_000, 5_000);

        let current_items = items
            .iter()
            .filter(|item| item.is_current)
            .map(|item| (item.page_index, item.page_label.to_string()))
            .collect::<Vec<_>>();

        assert_eq!(current_items, vec![(5_000, "5001".to_owned())]);
    }

    #[test]
    fn thumbnail_current_row_index_uses_absolute_position() {
        assert_eq!(thumbnail_current_row_index(10_000, 5_000, 8), 5_000);
        assert_eq!(thumbnail_current_row_index(10, 0, 3), 0);
        assert_eq!(thumbnail_current_row_index(10, 9, 3), 9);
        assert_eq!(thumbnail_current_row_index(10, 99, 3), 9);
        assert_eq!(thumbnail_current_row_index(0, 99, 3), 0);
    }
}
