//! Product budgets shared by broker validation and helper preflight.
use anyhow::{ensure, Context, Result};
use std::time::{Duration, Instant};

pub const MAX_PDF_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_PAGES: u32 = 10_000;
pub const MAX_DIMENSION: u32 = 4096;
pub const MAX_PIXELS: usize = 16 * 1024 * 1024;
pub const MAX_PIXEL_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_HELPER_MEMORY_BYTES: usize = 1024 * 1024 * 1024;
pub const MAX_PENDING_WORK: usize = 64;
pub const MAX_PENDING_PIXEL_BYTES: usize = 128 * 1024 * 1024;
pub const RESTART_WINDOW: Duration = Duration::from_secs(60);
pub const RESTART_BACKOFF: Duration = Duration::from_millis(500);
pub const MEMORY_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Handshake,
    Open,
    VisibleRender,
    AuxiliaryRender,
    Notes,
    Text,
    Shutdown,
}
impl Operation {
    pub fn deadline(self) -> Duration {
        #[cfg(debug_assertions)]
        if let Some(ms) = std::env::var("QUICK_PRESENTER_HELPER_TEST_DEADLINE_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|ms| (50..=30_000).contains(ms))
        {
            return Duration::from_millis(ms);
        }
        Duration::from_secs(match self {
            Self::Handshake => 5,
            Self::Open => 30,
            Self::VisibleRender => 5,
            Self::AuxiliaryRender => 10,
            Self::Notes | Self::Text => 5,
            Self::Shutdown => 1,
        })
    }
}

pub fn pixel_bytes(width: u32, height: u32) -> Result<usize> {
    ensure!(
        (1..=MAX_DIMENSION).contains(&width) && (1..=MAX_DIMENSION).contains(&height),
        "render dimension exceeds limit"
    );
    let pixels = usize::try_from(width)?
        .checked_mul(usize::try_from(height)?)
        .context("pixel count overflow")?;
    ensure!(pixels <= MAX_PIXELS, "decoded pixel count exceeds limit");
    let bytes = pixels.checked_mul(4).context("pixel byte count overflow")?;
    ensure!(bytes <= MAX_PIXEL_BYTES, "decoded pixel bytes exceed limit");
    Ok(bytes)
}

pub fn render_dimensions(
    width_points: f32,
    height_points: f32,
    target_width: i32,
) -> Result<(u32, u32)> {
    ensure!(
        width_points.is_finite()
            && height_points.is_finite()
            && width_points > 0.0
            && height_points > 0.0,
        "invalid page geometry"
    );
    let width = u32::try_from(target_width)?;
    ensure!(
        (1..=MAX_DIMENSION).contains(&width),
        "target width exceeds limit"
    );
    // Match PDFium-render's f32 scaling and rounding before native bitmap allocation.
    let scale = target_width as f32 / width_points;
    let output_width = (width_points * scale).round();
    let output_height = (height_points * scale).round();
    ensure!(
        output_width.is_finite()
            && output_height.is_finite()
            && output_width == width as f32
            && output_height >= 1.0
            && output_height <= MAX_DIMENSION as f32,
        "render geometry exceeds limit"
    );
    let height = output_height as u32;
    pixel_bytes(width, height)?;
    Ok((width, height))
}

#[derive(Default)]
pub struct RestartBudget {
    last_restart: Option<Instant>,
    suppressed: bool,
}
impl RestartBudget {
    pub fn take(&mut self, now: Instant) -> bool {
        if self.suppressed {
            return false;
        }
        if self
            .last_restart
            .is_some_and(|last| now.saturating_duration_since(last) < RESTART_WINDOW)
        {
            self.suppressed = true;
            return false;
        }
        self.last_restart = Some(now);
        true
    }
    pub fn suppress(&mut self) {
        self.suppressed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_pixels_and_geometry_reject_pathological_inputs() {
        assert_eq!(pixel_bytes(4096, 4096).unwrap(), MAX_PIXEL_BYTES);
        for (w, h) in [(0, 1), (4097, 1), (1, u32::MAX)] {
            assert!(pixel_bytes(w, h).is_err());
        }
        assert_eq!(render_dimensions(1600.0, 900.0, 1600).unwrap(), (1600, 900));
        for (w, h, target) in [
            (0.0, 1.0, 1600),
            (f32::NAN, 1.0, 1600),
            (1.0, f32::INFINITY, 1600),
            (1.0, 1e20, 1600),
            (1e-30, 1.0, 1600),
            (100.0, 100.0, -1),
        ] {
            assert!(render_dimensions(w, h, target).is_err());
        }
    }
    #[test]
    fn restart_budget_is_bounded_and_suppression_requires_explicit_new_session() {
        let now = Instant::now();
        let mut budget = RestartBudget::default();
        assert!(budget.take(now));
        assert!(!budget.take(now + RESTART_BACKOFF));
        assert!(!budget.take(now + RESTART_WINDOW * 2));
        assert!(RestartBudget::default().take(now));
        let mut spaced = RestartBudget::default();
        assert!(spaced.take(now));
        assert!(spaced.take(now + RESTART_WINDOW));
    }
}
