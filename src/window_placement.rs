use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};

pub fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux-x11"
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DisplayRect {
    pub origin: [i32; 2],
    pub extent: [u32; 2],
    pub scale: f64,
}
impl DisplayRect {
    pub fn valid(&self) -> bool {
        self.extent.iter().all(|v| *v > 0 && *v <= i32::MAX as u32)
            && self.scale.is_finite()
            && self.scale > 0.0
    }
    pub fn desktop_scale(&self) -> f64 {
        if cfg!(target_os = "macos") {
            1.0
        } else {
            self.scale
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlacementRecord {
    pub platform: String,
    pub source_display: DisplayRect,
    pub relative_center: [f64; 2],
    pub logical_inner_size: [f64; 2],
}
impl PlacementRecord {
    pub fn valid(&self) -> bool {
        self.platform == platform()
            && self.source_display.valid()
            && self
                .relative_center
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && self
                .logical_inner_size
                .iter()
                .all(|v| v.is_finite() && *v > 0.0 && *v <= i32::MAX as f64)
    }
    pub fn retarget(&mut self, target: DisplayRect) {
        self.source_display = target;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RestorePlan {
    pub target: usize,
    pub position: [i32; 2],
    pub logical_inner_size: [f64; 2],
}

pub fn plan_restore(
    saved: Option<&PlacementRecord>,
    displays: &[DisplayRect],
    fallback: usize,
    frame_logical: [f64; 2],
    default_size: [f64; 2],
) -> Option<RestorePlan> {
    if frame_logical.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return None;
    }
    let saved = saved.filter(|s| s.valid());
    let matches: Vec<_> = displays
        .iter()
        .enumerate()
        .filter(|(_, d)| {
            d.valid()
                && saved.is_some_and(|s| {
                    s.source_display.origin == d.origin
                        && s.source_display.extent == d.extent
                        && (s.source_display.scale - d.scale).abs() <= 0.001
                })
        })
        .map(|(i, _)| i)
        .collect();
    let target = if matches.len() == 1 {
        matches[0]
    } else {
        fallback
    };
    let display = displays.get(target).filter(|d| d.valid())?;
    let scale = display.desktop_scale();
    let size = saved.map(|s| s.logical_inner_size).unwrap_or(default_size);
    if size.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        return None;
    }
    let center = if matches.len() == 1 {
        saved?.relative_center
    } else {
        [0.5; 2]
    };
    let mut position = [0; 2];
    let mut logical_inner_size = [0.0; 2];
    for axis in 0..2 {
        let minimum = [640.0, 360.0][axis];
        let available = f64::from(display.extent[axis]) / scale - frame_logical[axis];
        let inner = size[axis].max(minimum).min(available.max(minimum));
        let outer = (inner + frame_logical[axis]) * scale;
        let origin = f64::from(display.origin[axis]);
        let maximum = origin + (f64::from(display.extent[axis]) - outer).max(0.0);
        let coordinate = (origin + center[axis] * f64::from(display.extent[axis]) - outer / 2.0)
            .clamp(origin, maximum)
            .round();
        if !outer.is_finite()
            || outer > i32::MAX as f64
            || coordinate < i32::MIN as f64
            || coordinate > i32::MAX as f64
        {
            return None;
        }
        position[axis] = coordinate as i32;
        logical_inner_size[axis] = inner;
    }
    Some(RestorePlan {
        target,
        position,
        logical_inner_size,
    })
}

#[derive(Default)]
pub struct SlidePlacementController {
    pub enabled: Cell<bool>,
    pub busy: Cell<bool>,
    pub restoring: Cell<bool>,
    pub visible: Cell<bool>,
    pub preserve_size: Cell<bool>,
    pub generation: Cell<u64>,
    pub capture_generation: Cell<u64>,
    pub remembered: RefCell<Option<PlacementRecord>>,
}
impl SlidePlacementController {
    pub fn initialize(&self, record: Option<PlacementRecord>) {
        self.enabled.set(true);
        self.visible.set(true);
        self.preserve_size
            .set(record.as_ref().is_some_and(PlacementRecord::valid));
        *self.remembered.borrow_mut() = record;
    }
    pub fn cancel(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
        self.busy.set(false);
        self.restoring.set(false);
    }
    pub fn suspend(&self) -> u64 {
        self.cancel();
        self.busy.set(true);
        self.generation.get()
    }
    pub fn current(&self, generation: u64) -> bool {
        self.enabled.get() && self.busy.get() && self.generation.get() == generation
    }
    pub fn can_capture(&self, fullscreen: bool, minimized: bool) -> bool {
        self.enabled.get() && self.visible.get() && !self.busy.get() && !fullscreen && !minimized
    }
    pub fn capture(&self, record: PlacementRecord) {
        if self.can_capture(false, false) && record.valid() {
            *self.remembered.borrow_mut() = Some(record);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn display(x: i32, scale: f64) -> DisplayRect {
        DisplayRect {
            origin: [x, 0],
            extent: [1920, 1080],
            scale,
        }
    }
    fn record() -> PlacementRecord {
        PlacementRecord {
            platform: platform().into(),
            source_display: display(-1920, 1.0),
            relative_center: [0.5, 0.5],
            logical_inner_size: [1024.0, 576.0],
        }
    }
    #[test]
    fn capture_excludes_fullscreen_minimized_and_disabled_windows() {
        let controller = SlidePlacementController::default();
        assert!(!controller.can_capture(false, false));
        controller.initialize(Some(record()));
        assert!(controller.can_capture(false, false));
        assert!(!controller.can_capture(true, false));
        assert!(!controller.can_capture(false, true));
        controller.visible.set(false);
        assert!(!controller.can_capture(false, false));
    }
    #[test]
    fn fullscreen_retarget_preserves_normal_size_and_anchor() {
        let mut saved = record();
        let size = saved.logical_inner_size;
        let anchor = saved.relative_center;
        saved.retarget(display(0, 2.0));
        assert_eq!(saved.logical_inner_size, size);
        assert_eq!(saved.relative_center, anchor);
        assert_eq!(saved.source_display, display(0, 2.0));
        let plan = plan_restore(
            Some(&saved),
            &[saved.source_display.clone()],
            0,
            [0.0; 2],
            [800.0, 600.0],
        )
        .unwrap();
        assert!(plan.logical_inner_size[0] <= size[0]);
        assert!(plan.logical_inner_size[1] <= size[1]);
    }
    #[test]
    fn invalid_record_falls_back_to_default_without_nan_coordinates() {
        let mut saved = record();
        saved.relative_center[0] = f64::NAN;
        assert!(!saved.valid());
        let plan = plan_restore(
            Some(&saved),
            &[display(0, 1.0)],
            0,
            [0.0; 2],
            [800.0, 600.0],
        )
        .unwrap();
        assert_eq!(plan.logical_inner_size, [800.0, 600.0]);
        saved = record();
        saved.logical_inner_size[1] = 0.0;
        assert!(!saved.valid());
        saved = record();
        saved.platform = "other".into();
        assert!(!saved.valid());
    }
    #[test]
    fn mixed_scale_preserves_logical_size_when_display_has_room() {
        let saved = record();
        let target = DisplayRect {
            origin: [0, 0],
            extent: [3840, 2160],
            scale: 2.0,
        };
        let plan = plan_restore(Some(&saved), &[target], 0, [0.0; 2], [800.0, 600.0]).unwrap();
        assert_eq!(plan.logical_inner_size, saved.logical_inner_size);
    }

    #[test]
    fn unchanged_display_restores_size_and_negative_origin() {
        let plan = plan_restore(
            Some(&record()),
            &[display(0, 1.0), display(-1920, 1.0)],
            0,
            [0.0, 28.0],
            [800.0, 600.0],
        )
        .unwrap();
        assert_eq!(plan.target, 1);
        assert_eq!(plan.position, [-1472, 238]);
        assert_eq!(plan.logical_inner_size, [1024.0, 576.0]);
    }
    #[test]
    fn stale_and_ambiguous_geometry_use_fallback() {
        for displays in [
            vec![display(0, 1.0)],
            vec![display(-1920, 1.0), display(-1920, 1.0), display(0, 1.0)],
        ] {
            let fallback = displays.len() - 1;
            assert_eq!(
                plan_restore(
                    Some(&record()),
                    &displays,
                    fallback,
                    [0.0; 2],
                    [800.0, 600.0]
                )
                .unwrap()
                .target,
                fallback
            );
        }
    }
    #[test]
    fn large_sizes_and_edge_centers_remain_reachable() {
        let mut saved = record();
        saved.logical_inner_size = [10000.0; 2];
        saved.relative_center = [1.0; 2];
        let plan = plan_restore(
            Some(&saved),
            &[display(-1920, 1.0)],
            0,
            [0.0, 28.0],
            [800.0, 600.0],
        )
        .unwrap();
        assert_eq!(plan.position, [-1920, 0]);
        assert_eq!(plan.logical_inner_size, [1920.0, 1052.0]);
    }
    #[test]
    fn small_display_keeps_minimum_at_origin() {
        let tiny = DisplayRect {
            origin: [300, -100],
            extent: [320, 200],
            scale: 1.0,
        };
        let plan = plan_restore(None, &[tiny], 0, [0.0; 2], [800.0, 600.0]).unwrap();
        assert_eq!(plan.position, [300, -100]);
        assert_eq!(plan.logical_inner_size, [640.0, 360.0]);
    }
    #[test]
    fn rejects_invalid_topology_and_overflow() {
        assert!(plan_restore(None, &[], 0, [0.0; 2], [800.0, 600.0]).is_none());
        let mut d = display(i32::MAX, 1.0);
        assert!(plan_restore(None, &[d.clone()], 0, [0.0; 2], [800.0, 600.0]).is_none());
        d.scale = f64::NAN;
        assert!(plan_restore(None, &[d], 0, [0.0; 2], [800.0, 600.0]).is_none());
    }
    #[test]
    fn suspended_hidden_and_cancelled_operations_do_not_capture() {
        let controller = SlidePlacementController::default();
        controller.initialize(Some(record()));
        let generation = controller.suspend();
        assert!(controller.current(generation));
        let mut changed = record();
        changed.relative_center = [0.1; 2];
        controller.capture(changed.clone());
        assert_eq!(*controller.remembered.borrow(), Some(record()));
        controller.cancel();
        assert!(!controller.current(generation));
        controller.visible.set(false);
        controller.capture(changed.clone());
        assert_eq!(*controller.remembered.borrow(), Some(record()));
        controller.visible.set(true);
        controller.capture(changed.clone());
        assert_eq!(*controller.remembered.borrow(), Some(changed));
    }
}
