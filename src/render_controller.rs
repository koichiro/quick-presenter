use crate::{
    app_state::AppState,
    presentation::PageSnapshot,
    render_scheduler::{RenderPriority, RenderSessionId},
    rendering::{
        presentation_preload_order, thumbnail_window_indices, RenderPurpose, RenderRequest,
    },
};

pub const CURRENT_RENDER_WIDTH: i32 = 1600;
pub const PREVIEW_RENDER_WIDTH: i32 = 600;
pub const THUMBNAIL_RENDER_WIDTH: i32 = 180;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct RenderPlanItem {
    pub request: RenderRequest,
    pub priority: RenderPriority,
}

pub fn visible_page_render_plan(snapshot: &PageSnapshot) -> Vec<RenderPlanItem> {
    let mut plan = vec![RenderPlanItem {
        request: RenderRequest {
            page_index: snapshot.current_index,
            width: CURRENT_RENDER_WIDTH,
            purpose: RenderPurpose::CurrentSlide,
        },
        priority: RenderPriority::BlockingVisible,
    }];

    if let Some(page_index) = snapshot.next_index {
        plan.push(RenderPlanItem {
            request: RenderRequest {
                page_index,
                width: PREVIEW_RENDER_WIDTH,
                purpose: RenderPurpose::NextPreview,
            },
            priority: RenderPriority::VisibleAux,
        });
    }

    plan
}

pub fn presentation_preload_render_plan(
    snapshot: &PageSnapshot,
    presentation_cache_radius: u32,
) -> Vec<RenderPlanItem> {
    presentation_preload_order(
        snapshot.current_index,
        snapshot.total_pages,
        presentation_cache_radius,
    )
    .into_iter()
    .map(|page_index| RenderPlanItem {
        request: RenderRequest {
            page_index,
            width: CURRENT_RENDER_WIDTH,
            purpose: RenderPurpose::CurrentSlide,
        },
        priority: RenderPriority::Warm,
    })
    .collect()
}

pub fn thumbnail_render_plan(
    snapshot: &PageSnapshot,
    thumbnail_cache_radius: u32,
) -> Vec<RenderPlanItem> {
    thumbnail_window_indices(
        snapshot.current_index,
        snapshot.total_pages,
        thumbnail_cache_radius,
    )
    .into_iter()
    .map(|page_index| RenderPlanItem {
        request: RenderRequest {
            page_index,
            width: THUMBNAIL_RENDER_WIDTH,
            purpose: RenderPurpose::Thumbnail,
        },
        priority: RenderPriority::Background,
    })
    .collect()
}

pub fn thumbnail_visible_range_render_plan(
    total_pages: u32,
    first_visible_index: i32,
    last_visible_index: i32,
    lookahead: u32,
) -> Vec<RenderPlanItem> {
    let Some((first, last)) = thumbnail_visible_range(
        total_pages,
        first_visible_index,
        last_visible_index,
        lookahead,
    ) else {
        return Vec::new();
    };

    (first..=last)
        .map(|page_index| RenderPlanItem {
            request: RenderRequest {
                page_index,
                width: THUMBNAIL_RENDER_WIDTH,
                purpose: RenderPurpose::Thumbnail,
            },
            priority: RenderPriority::Background,
        })
        .collect()
}

fn thumbnail_visible_range(
    total_pages: u32,
    first_visible_index: i32,
    last_visible_index: i32,
    lookahead: u32,
) -> Option<(u32, u32)> {
    if total_pages == 0 || first_visible_index < 0 || last_visible_index < 0 {
        return None;
    }

    let first = u32::try_from(first_visible_index).ok()?;
    let last = u32::try_from(last_visible_index).ok()?;
    if first >= total_pages {
        return None;
    }

    let last_page = total_pages.saturating_sub(1);
    let start = first.saturating_sub(lookahead);
    let end = last.min(last_page).saturating_add(lookahead).min(last_page);
    Some((start, end.max(start)))
}

pub fn enqueue_render_plan_if_missing(
    state: &AppState,
    plan: impl IntoIterator<Item = RenderPlanItem>,
) {
    for item in plan {
        enqueue_render_if_missing(state, item.request, item.priority);
    }
}

fn enqueue_render_if_missing(state: &AppState, request: RenderRequest, priority: RenderPriority) {
    let Some(session_id) = render_enqueue_session(
        state.render_cache.peek(request).is_some(),
        state.render_sessions.current_session(),
        state.render_scheduler.is_some(),
    ) else {
        return;
    };

    if let Some(scheduler) = state.render_scheduler.as_ref() {
        scheduler.render_page(session_id, request, priority);
    }
}

fn render_enqueue_session(
    cache_hit: bool,
    current_session: Option<RenderSessionId>,
    has_scheduler: bool,
) -> Option<RenderSessionId> {
    if cache_hit || !has_scheduler {
        return None;
    }

    current_session
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(current_index: u32, total_pages: u32) -> PageSnapshot {
        PageSnapshot {
            title: "Deck".to_owned(),
            current_index,
            current_number: current_index + 1,
            total_pages,
            next_index: (current_index + 1 < total_pages).then_some(current_index + 1),
            page_label: format!("{} / {}", current_index + 1, total_pages),
        }
    }

    #[test]
    fn visible_plan_includes_current_and_next_preview() {
        let plan = visible_page_render_plan(&snapshot(1, 3));

        assert_eq!(
            plan,
            vec![
                RenderPlanItem {
                    request: RenderRequest {
                        page_index: 1,
                        width: CURRENT_RENDER_WIDTH,
                        purpose: RenderPurpose::CurrentSlide,
                    },
                    priority: RenderPriority::BlockingVisible,
                },
                RenderPlanItem {
                    request: RenderRequest {
                        page_index: 2,
                        width: PREVIEW_RENDER_WIDTH,
                        purpose: RenderPurpose::NextPreview,
                    },
                    priority: RenderPriority::VisibleAux,
                },
            ]
        );
    }

    #[test]
    fn visible_plan_omits_next_preview_on_last_page() {
        let plan = visible_page_render_plan(&snapshot(2, 3));

        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].request.page_index, 2);
    }

    #[test]
    fn preload_plan_uses_warm_current_slide_requests() {
        let plan = presentation_preload_render_plan(&snapshot(2, 5), 2);

        assert_eq!(
            plan.iter()
                .map(|item| (item.request.page_index, item.request.purpose, item.priority))
                .collect::<Vec<_>>(),
            vec![
                (3, RenderPurpose::CurrentSlide, RenderPriority::Warm),
                (1, RenderPurpose::CurrentSlide, RenderPriority::Warm),
                (4, RenderPurpose::CurrentSlide, RenderPriority::Warm),
                (0, RenderPurpose::CurrentSlide, RenderPriority::Warm),
            ]
        );
    }

    #[test]
    fn thumbnail_plan_is_clamped_to_nearby_pages() {
        let plan = thumbnail_render_plan(&snapshot(0, 10), 2);

        assert_eq!(
            plan.iter()
                .map(|item| item.request.page_index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert!(plan
            .iter()
            .all(|item| item.request.purpose == RenderPurpose::Thumbnail));
    }

    #[test]
    fn thumbnail_visible_range_plan_expands_visible_rows_with_lookahead() {
        let plan = thumbnail_visible_range_render_plan(100, 20, 24, 3);

        assert_eq!(
            plan.iter()
                .map(|item| item.request.page_index)
                .collect::<Vec<_>>(),
            (17..=27).collect::<Vec<_>>()
        );
        assert!(plan
            .iter()
            .all(|item| item.request.purpose == RenderPurpose::Thumbnail));
    }

    #[test]
    fn thumbnail_visible_range_plan_clamps_to_document_edges() {
        let first = thumbnail_visible_range_render_plan(10, 0, 2, 4);
        let last = thumbnail_visible_range_render_plan(10, 8, 12, 4);

        assert_eq!(
            first
                .iter()
                .map(|item| item.request.page_index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4, 5, 6]
        );
        assert_eq!(
            last.iter()
                .map(|item| item.request.page_index)
                .collect::<Vec<_>>(),
            vec![4, 5, 6, 7, 8, 9]
        );
    }

    #[test]
    fn thumbnail_visible_range_plan_ignores_invalid_ranges() {
        assert!(thumbnail_visible_range_render_plan(0, 0, 2, 4).is_empty());
        assert!(thumbnail_visible_range_render_plan(10, -1, 2, 4).is_empty());
        assert!(thumbnail_visible_range_render_plan(10, 99, 100, 4).is_empty());
    }

    #[test]
    fn enqueue_decision_skips_cache_hits() {
        assert_eq!(
            render_enqueue_session(true, Some(RenderSessionId(7)), true),
            None
        );
    }

    #[test]
    fn enqueue_decision_skips_missing_session() {
        assert_eq!(render_enqueue_session(false, None, true), None);
    }

    #[test]
    fn enqueue_decision_skips_missing_scheduler() {
        assert_eq!(
            render_enqueue_session(false, Some(RenderSessionId(7)), false),
            None
        );
    }

    #[test]
    fn enqueue_decision_returns_active_session_for_cache_miss() {
        assert_eq!(
            render_enqueue_session(false, Some(RenderSessionId(7)), true),
            Some(RenderSessionId(7))
        );
    }
}
