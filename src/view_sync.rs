use std::rc::Rc;
use std::time::Instant;

use slint::{ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel, Weak};

use crate::{
    app_state::{AppState, ThumbnailState},
    clock::current_clock_label,
    errors::PresenterMessage,
    presentation::PageSnapshot,
    rendering::{RenderCache, RenderPurpose, RenderRequest, RenderedPage},
    window_controller::AppWindowRefs,
    PresenterWindow, ThumbnailItem,
};

pub const CURRENT_RENDER_WIDTH: i32 = 1600;
pub const PREVIEW_RENDER_WIDTH: i32 = 600;
pub const THUMBNAIL_RENDER_WIDTH: i32 = 180;

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
    let current_request = RenderRequest {
        page_index: snapshot.current_index,
        width: CURRENT_RENDER_WIDTH,
        purpose: RenderPurpose::CurrentSlide,
    };
    let current = state
        .render_cache
        .peek(current_request)
        .unwrap_or_else(placeholder_slide);
    let next = snapshot.next_index.and_then(|page_index| {
        state.render_cache.peek(RenderRequest {
            page_index,
            width: PREVIEW_RENDER_WIDTH,
            purpose: RenderPurpose::NextPreview,
        })
    });
    let next_placeholder = placeholder_slide();

    if let Some(presenter) = windows.presenter.upgrade() {
        presenter.set_current_page_image(current.image.clone());
        presenter.set_current_page_aspect_ratio(current.aspect_ratio);
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
        presenter.set_status_text(presenter_status_text(state).into());
        presenter.set_current_page_index(presenter_page_index(snapshot.current_index));
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
        slide.set_page_aspect_ratio(current.aspect_ratio);
        slide.set_page_image(if state.black_screen.is_active() {
            black_slide_image()
        } else {
            current.image
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

pub fn presenter_page_index(page_index: u32) -> i32 {
    i32::try_from(page_index).unwrap_or(i32::MAX)
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
