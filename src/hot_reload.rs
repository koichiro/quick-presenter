use std::{
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
        Arc,
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use notify::{Config, Event, EventKind, PollWatcher, RecommendedWatcher, RecursiveMode, Watcher};

use crate::render_scheduler::RenderSessionId;

pub const HOT_RELOAD_DEBOUNCE: Duration = Duration::from_millis(300);
pub const HOT_RELOAD_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(50);
pub const HOT_RELOAD_SLOW_DELAY: Duration = Duration::from_secs(2);
pub const HOT_RELOAD_SUCCESS_NOTICE: Duration = Duration::from_secs(2);
pub const HOT_RELOAD_RETRY_DELAYS: [Duration; 4] = [
    Duration::from_millis(100),
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_secs(1),
];
pub const WATCHER_RECOVERY_DELAYS: [Duration; 5] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(30),
];

const WATCH_EVENT_CAPACITY: usize = 64;
const POLL_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WatchTarget {
    path: PathBuf,
    parent: PathBuf,
}

impl WatchTarget {
    pub fn new(path: PathBuf) -> Result<Self> {
        let path = absolute_lexical_path(path)?;
        let parent = path
            .parent()
            .context("PDF path has no parent directory")?
            .to_path_buf();
        Ok(Self { path, parent })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn parent(&self) -> &Path {
        &self.parent
    }

    fn matches(&self, path: &Path) -> bool {
        absolute_lexical_path(path.to_path_buf())
            .map(|path| path == self.path)
            .unwrap_or(false)
    }
}

fn absolute_lexical_path(path: PathBuf) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .context("failed to resolve current directory")?
            .join(path)
    };

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if normalized.file_name().is_some() {
                    normalized.pop();
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WatchSignal {
    Paths(Vec<PathBuf>),
    Reconcile,
    Failed(String),
}

enum WatchBackend {
    Native(RecommendedWatcher),
    Poll(PollWatcher),
}

trait PdfWatchBackend {
    fn watch(&mut self, path: &Path) -> notify::Result<()>;
    fn unwatch(&mut self, path: &Path) -> notify::Result<()>;
}

impl PdfWatchBackend for WatchBackend {
    fn watch(&mut self, path: &Path) -> notify::Result<()> {
        match self {
            Self::Native(watcher) => watcher.watch(path, RecursiveMode::NonRecursive),
            Self::Poll(watcher) => watcher.watch(path, RecursiveMode::NonRecursive),
        }
    }

    fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
        match self {
            Self::Native(watcher) => watcher.unwatch(path),
            Self::Poll(watcher) => watcher.unwatch(path),
        }
    }
}

pub struct PdfWatcher {
    backend: Box<dyn PdfWatchBackend>,
    receiver: Receiver<WatchSignal>,
    overflowed: Arc<AtomicBool>,
    target: Option<WatchTarget>,
}

impl PdfWatcher {
    pub fn new() -> Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(WATCH_EVENT_CAPACITY);
        let overflowed = Arc::new(AtomicBool::new(false));
        let backend = create_native_watcher(sender.clone(), Arc::clone(&overflowed))
            .map(WatchBackend::Native)
            .or_else(|native_error| {
                create_poll_watcher(sender, Arc::clone(&overflowed))
                    .map(WatchBackend::Poll)
                    .map_err(|poll_error| {
                        anyhow::anyhow!(
                            "native watcher failed: {native_error}; polling watcher failed: {poll_error}"
                        )
                    })
            })?;

        Ok(Self {
            backend: Box::new(backend),
            receiver,
            overflowed,
            target: None,
        })
    }

    #[cfg(test)]
    fn with_backend_for_test(backend: Box<dyn PdfWatchBackend>) -> Self {
        let (_sender, receiver) = mpsc::sync_channel(WATCH_EVENT_CAPACITY);
        Self {
            backend,
            receiver,
            overflowed: Arc::new(AtomicBool::new(false)),
            target: None,
        }
    }

    pub fn replace_target(&mut self, target: Option<WatchTarget>) -> Result<()> {
        let old_parent = self.target.as_ref().map(WatchTarget::parent);
        let new_parent = target.as_ref().map(WatchTarget::parent);

        if old_parent != new_parent {
            if let Some(parent) = new_parent {
                self.backend
                    .watch(parent)
                    .with_context(|| format!("failed to watch {}", parent.display()))?;
            }
            if let Some(parent) = old_parent {
                if let Err(error) = self.backend.unwatch(parent) {
                    if let Some(new_parent) = new_parent {
                        let _ = self.backend.unwatch(new_parent);
                    }
                    return Err(error)
                        .with_context(|| format!("failed to stop watching {}", parent.display()));
                }
            }
        }

        self.target = target;
        Ok(())
    }

    pub fn drain(&self) -> Vec<WatchSignal> {
        let mut signals = Vec::new();
        while let Ok(signal) = self.receiver.try_recv() {
            signals.push(signal);
        }
        if self.overflowed.swap(false, Ordering::AcqRel) {
            signals.push(WatchSignal::Reconcile);
        }
        signals
    }
}

fn create_native_watcher(
    sender: SyncSender<WatchSignal>,
    overflowed: Arc<AtomicBool>,
) -> notify::Result<RecommendedWatcher> {
    RecommendedWatcher::new(event_handler(sender, overflowed), Config::default())
}

fn create_poll_watcher(
    sender: SyncSender<WatchSignal>,
    overflowed: Arc<AtomicBool>,
) -> notify::Result<PollWatcher> {
    PollWatcher::new(
        event_handler(sender, overflowed),
        Config::default().with_poll_interval(POLL_INTERVAL),
    )
}

fn event_handler(
    sender: SyncSender<WatchSignal>,
    overflowed: Arc<AtomicBool>,
) -> impl FnMut(notify::Result<Event>) + Send + 'static {
    move |event| {
        let signal = match event {
            Ok(event) if event.need_rescan() || event.paths.is_empty() => WatchSignal::Reconcile,
            Ok(event) if matches!(event.kind, EventKind::Access(_)) => return,
            Ok(event) => WatchSignal::Paths(event.paths),
            Err(error) => WatchSignal::Failed(error.to_string()),
        };
        if matches!(sender.try_send(signal), Err(TrySendError::Full(_))) {
            overflowed.store(true, Ordering::Release);
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HotReloadPhase {
    Idle,
    Debouncing,
    Preparing {
        session_id: RenderSessionId,
        revision: u64,
    },
    Failed {
        revision: u64,
    },
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PreparationOutcome {
    Ignored,
    Succeeded,
    RetryScheduled,
    Failed,
    Superseded,
}

#[derive(Debug, Default)]
pub struct WatcherRecoveryState {
    attempts: usize,
    next_retry_at: Option<Instant>,
}

impl WatcherRecoveryState {
    pub fn record_failure(&mut self, now: Instant) {
        let delay = WATCHER_RECOVERY_DELAYS[self.attempts.min(WATCHER_RECOVERY_DELAYS.len() - 1)];
        self.attempts = self.attempts.saturating_add(1);
        self.next_retry_at = Some(now + delay);
    }

    pub fn retry_is_due(&self, now: Instant) -> bool {
        self.next_retry_at.is_some_and(|deadline| now >= deadline)
    }

    pub fn record_success(&mut self) {
        self.attempts = 0;
        self.next_retry_at = None;
    }
}

#[derive(Debug)]
pub struct HotReloadState {
    target: Option<WatchTarget>,
    revision: u64,
    phase: HotReloadPhase,
    deadline: Option<Instant>,
    manual_open_pending: bool,
    retry_attempt: usize,
    success_notice_until: Option<Instant>,
    preparing_started_at: Option<Instant>,
    slow_status_shown: bool,
}

impl Default for HotReloadState {
    fn default() -> Self {
        Self {
            target: None,
            revision: 0,
            phase: HotReloadPhase::Idle,
            deadline: None,
            manual_open_pending: false,
            retry_attempt: 0,
            success_notice_until: None,
            preparing_started_at: None,
            slow_status_shown: false,
        }
    }
}

impl HotReloadState {
    pub fn target(&self) -> Option<&WatchTarget> {
        self.target.as_ref()
    }

    pub fn phase(&self) -> HotReloadPhase {
        self.phase
    }

    pub fn replace_target(&mut self, target: WatchTarget) {
        self.target = Some(target);
        self.revision = self.revision.wrapping_add(1);
        self.phase = HotReloadPhase::Idle;
        self.deadline = None;
        self.manual_open_pending = false;
        self.retry_attempt = 0;
        self.success_notice_until = None;
        self.preparing_started_at = None;
        self.slow_status_shown = false;
    }

    pub fn begin_manual_open(&mut self) {
        self.manual_open_pending = true;
        self.revision = self.revision.wrapping_add(1);
        self.phase = HotReloadPhase::Idle;
        self.deadline = None;
        self.success_notice_until = None;
        self.preparing_started_at = None;
        self.slow_status_shown = false;
    }

    pub fn finish_manual_open_without_replacement(&mut self) {
        self.manual_open_pending = false;
    }

    pub fn cancel_pending_reload(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.phase = HotReloadPhase::Idle;
        self.deadline = None;
        self.preparing_started_at = None;
        self.slow_status_shown = false;
    }

    pub fn observe(&mut self, signal: &WatchSignal, now: Instant) -> bool {
        if self.manual_open_pending || !self.is_relevant(signal) {
            return false;
        }

        self.revision = self.revision.wrapping_add(1);
        self.retry_attempt = 0;
        if !matches!(self.phase, HotReloadPhase::Preparing { .. }) {
            self.phase = HotReloadPhase::Debouncing;
            self.deadline = Some(now + HOT_RELOAD_DEBOUNCE);
        }
        true
    }

    pub fn due_revision(&self, now: Instant) -> Option<u64> {
        (self.phase == HotReloadPhase::Debouncing
            && self.deadline.is_some_and(|deadline| now >= deadline))
        .then_some(self.revision)
    }

    pub fn begin_preparing(
        &mut self,
        session_id: RenderSessionId,
        revision: u64,
        now: Instant,
    ) -> bool {
        if self.phase != HotReloadPhase::Debouncing || self.revision != revision {
            return false;
        }
        self.phase = HotReloadPhase::Preparing {
            session_id,
            revision,
        };
        self.deadline = None;
        self.preparing_started_at = Some(now);
        self.slow_status_shown = false;
        true
    }

    pub fn mark_preparing_slow(&mut self, now: Instant) -> bool {
        if !matches!(self.phase, HotReloadPhase::Preparing { .. }) || self.slow_status_shown {
            return false;
        }
        if now
            .checked_duration_since(self.preparing_started_at.unwrap_or(now))
            .unwrap_or_default()
            < HOT_RELOAD_SLOW_DELAY
        {
            return false;
        }
        self.slow_status_shown = true;
        true
    }

    pub fn accepts_prepared(&self, session_id: RenderSessionId, revision: u64) -> bool {
        self.phase
            == HotReloadPhase::Preparing {
                session_id,
                revision,
            }
            && self.revision == revision
            && !self.manual_open_pending
    }

    pub fn finish_preparing(
        &mut self,
        session_id: RenderSessionId,
        revision: u64,
        succeeded: bool,
        now: Instant,
    ) -> PreparationOutcome {
        self.finish_preparing_with_retry(session_id, revision, succeeded, true, now)
    }
    pub fn finish_preparing_with_retry(
        &mut self,
        session_id: RenderSessionId,
        revision: u64,
        succeeded: bool,
        retryable: bool,
        now: Instant,
    ) -> PreparationOutcome {
        if self.phase
            != (HotReloadPhase::Preparing {
                session_id,
                revision,
            })
        {
            return PreparationOutcome::Ignored;
        }

        self.preparing_started_at = None;
        self.slow_status_shown = false;

        if self.revision != revision {
            self.phase = HotReloadPhase::Debouncing;
            self.deadline = Some(now + HOT_RELOAD_DEBOUNCE);
            PreparationOutcome::Superseded
        } else if succeeded {
            self.phase = HotReloadPhase::Idle;
            self.deadline = None;
            self.retry_attempt = 0;
            PreparationOutcome::Succeeded
        } else if let Some(delay) = HOT_RELOAD_RETRY_DELAYS
            .get(self.retry_attempt)
            .copied()
            .filter(|_| retryable)
        {
            self.retry_attempt = self.retry_attempt.saturating_add(1);
            self.phase = HotReloadPhase::Debouncing;
            self.deadline = Some(now + delay);
            PreparationOutcome::RetryScheduled
        } else {
            self.phase = HotReloadPhase::Failed { revision };
            self.deadline = None;
            PreparationOutcome::Failed
        }
    }

    pub fn start_success_notice(&mut self, now: Instant) {
        self.success_notice_until = Some(now + HOT_RELOAD_SUCCESS_NOTICE);
    }

    pub fn success_notice_active(&self, now: Instant) -> bool {
        self.success_notice_until
            .is_some_and(|deadline| now < deadline)
    }

    pub fn take_expired_success_notice(&mut self, now: Instant) -> bool {
        if !self
            .success_notice_until
            .is_some_and(|deadline| now >= deadline)
        {
            return false;
        }
        self.success_notice_until = None;
        true
    }

    pub fn clear_success_notice(&mut self) {
        self.success_notice_until = None;
    }

    fn is_relevant(&self, signal: &WatchSignal) -> bool {
        let Some(target) = self.target.as_ref() else {
            return false;
        };
        match signal {
            WatchSignal::Paths(paths) => paths.iter().any(|path| target.matches(path)),
            WatchSignal::Reconcile => true,
            WatchSignal::Failed(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    #[derive(Debug, Clone, Eq, PartialEq)]
    enum WatchOperation {
        Watch(PathBuf),
        Unwatch(PathBuf),
    }

    struct FakeWatchBackend {
        operations: Rc<RefCell<Vec<WatchOperation>>>,
    }

    impl PdfWatchBackend for FakeWatchBackend {
        fn watch(&mut self, path: &Path) -> notify::Result<()> {
            self.operations
                .borrow_mut()
                .push(WatchOperation::Watch(path.to_path_buf()));
            Ok(())
        }

        fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
            self.operations
                .borrow_mut()
                .push(WatchOperation::Unwatch(path.to_path_buf()));
            Ok(())
        }
    }

    fn fake_watcher() -> (PdfWatcher, Rc<RefCell<Vec<WatchOperation>>>) {
        let operations = Rc::new(RefCell::new(Vec::new()));
        let watcher = PdfWatcher::with_backend_for_test(Box::new(FakeWatchBackend {
            operations: Rc::clone(&operations),
        }));
        (watcher, operations)
    }

    fn target() -> WatchTarget {
        WatchTarget::new(slides_dir().join("deck.pdf")).unwrap()
    }

    fn slides_dir() -> PathBuf {
        absolute_lexical_path(std::env::temp_dir().join("quick-presenter-hot-reload-slides"))
            .unwrap()
    }

    fn other_dir() -> PathBuf {
        absolute_lexical_path(std::env::temp_dir().join("quick-presenter-hot-reload-other"))
            .unwrap()
    }

    fn changed(path: PathBuf) -> WatchSignal {
        WatchSignal::Paths(vec![path])
    }

    fn changed_target() -> WatchSignal {
        changed(slides_dir().join("deck.pdf"))
    }

    #[test]
    fn target_normalizes_relative_components_without_resolving_the_file() {
        let target = WatchTarget::new(slides_dir().join("work/../deck.pdf")).unwrap();

        assert_eq!(target.path(), slides_dir().join("deck.pdf"));
        assert_eq!(target.parent(), slides_dir());
    }

    #[test]
    fn watcher_reuses_parent_when_only_target_file_changes() {
        let (mut watcher, operations) = fake_watcher();
        watcher.replace_target(Some(target())).unwrap();
        watcher
            .replace_target(Some(
                WatchTarget::new(slides_dir().join("other.pdf")).unwrap(),
            ))
            .unwrap();

        assert_eq!(
            operations.borrow().as_slice(),
            &[WatchOperation::Watch(slides_dir())]
        );
    }

    #[test]
    fn watcher_registers_new_parent_before_removing_old_parent() {
        let (mut watcher, operations) = fake_watcher();
        watcher.replace_target(Some(target())).unwrap();
        watcher
            .replace_target(Some(
                WatchTarget::new(other_dir().join("deck.pdf")).unwrap(),
            ))
            .unwrap();

        assert_eq!(
            operations.borrow().as_slice(),
            &[
                WatchOperation::Watch(slides_dir()),
                WatchOperation::Watch(other_dir()),
                WatchOperation::Unwatch(slides_dir()),
            ]
        );
    }

    #[test]
    fn relevant_event_starts_debounce() {
        let now = Instant::now();
        let mut state = HotReloadState::default();
        state.replace_target(target());

        assert!(state.observe(&changed_target(), now));
        assert_eq!(state.phase(), HotReloadPhase::Debouncing);
        assert_eq!(state.due_revision(now), None);
        assert_eq!(state.due_revision(now + HOT_RELOAD_DEBOUNCE), Some(2));
    }

    #[test]
    fn unrelated_sibling_event_is_ignored() {
        let mut state = HotReloadState::default();
        state.replace_target(target());

        assert!(!state.observe(&changed(slides_dir().join("notes.txt")), Instant::now()));
        assert_eq!(state.phase(), HotReloadPhase::Idle);
    }

    #[test]
    fn event_burst_extends_debounce_and_coalesces_revision() {
        let now = Instant::now();
        let mut state = HotReloadState::default();
        state.replace_target(target());
        state.observe(&changed_target(), now);
        state.observe(&changed_target(), now + Duration::from_millis(200));

        assert_eq!(state.due_revision(now + HOT_RELOAD_DEBOUNCE), None);
        assert_eq!(
            state.due_revision(now + Duration::from_millis(500)),
            Some(3)
        );
    }

    #[test]
    fn reconcile_signal_is_relevant_for_active_target() {
        let mut state = HotReloadState::default();
        state.replace_target(target());

        assert!(state.observe(&WatchSignal::Reconcile, Instant::now()));
    }

    #[test]
    fn new_event_during_prepare_makes_candidate_stale() {
        let now = Instant::now();
        let mut state = HotReloadState::default();
        state.replace_target(target());
        state.observe(&changed_target(), now);
        let revision = state.due_revision(now + HOT_RELOAD_DEBOUNCE).unwrap();
        assert!(state.begin_preparing(RenderSessionId(3), revision, now));

        state.observe(&changed_target(), now + HOT_RELOAD_DEBOUNCE);

        assert!(!state.accepts_prepared(RenderSessionId(3), revision));
        assert_eq!(
            state.finish_preparing(
                RenderSessionId(3),
                revision,
                true,
                now + HOT_RELOAD_DEBOUNCE
            ),
            PreparationOutcome::Superseded
        );
        assert_eq!(state.phase(), HotReloadPhase::Debouncing);
    }

    #[test]
    fn failed_prepare_retries_then_waits_for_later_event() {
        let now = Instant::now();
        let mut state = HotReloadState::default();
        state.replace_target(target());
        state.observe(&changed_target(), now);
        let mut attempt_at = now + HOT_RELOAD_DEBOUNCE;
        let revision = state.due_revision(attempt_at).unwrap();

        for (attempt, delay) in HOT_RELOAD_RETRY_DELAYS.iter().enumerate() {
            let session = RenderSessionId(4 + attempt as u64);
            assert!(state.begin_preparing(session, revision, attempt_at));
            assert_eq!(
                state.finish_preparing(session, revision, false, attempt_at),
                PreparationOutcome::RetryScheduled
            );
            attempt_at += *delay;
            assert_eq!(state.due_revision(attempt_at), Some(revision));
        }

        let final_session = RenderSessionId(99);
        assert!(state.begin_preparing(final_session, revision, attempt_at));
        assert_eq!(
            state.finish_preparing(final_session, revision, false, attempt_at),
            PreparationOutcome::Failed
        );
        assert_eq!(state.phase(), HotReloadPhase::Failed { revision });

        assert!(state.observe(&changed_target(), now + Duration::from_secs(1)));
        assert_eq!(state.phase(), HotReloadPhase::Debouncing);
    }

    #[test]
    fn helper_failure_stops_reload_retries_until_a_new_file_revision() {
        let now = Instant::now();
        let mut state = HotReloadState::default();
        state.replace_target(target());
        state.observe(&changed_target(), now);
        let due = now + HOT_RELOAD_DEBOUNCE;
        let revision = state.due_revision(due).unwrap();
        assert!(state.begin_preparing(RenderSessionId(9), revision, due));
        assert_eq!(
            state.finish_preparing_with_retry(RenderSessionId(9), revision, false, false, due),
            PreparationOutcome::Failed
        );
        assert!(state.due_revision(due + Duration::from_secs(60)).is_none());
        state.observe(&changed_target(), due + Duration::from_secs(60));
        assert!(state
            .due_revision(due + Duration::from_secs(60) + HOT_RELOAD_DEBOUNCE)
            .is_some());
    }

    #[test]
    fn success_notice_expires_once() {
        let now = Instant::now();
        let mut state = HotReloadState::default();
        state.start_success_notice(now);

        assert!(state.success_notice_active(now));
        assert!(!state.take_expired_success_notice(now));
        assert!(state.take_expired_success_notice(now + HOT_RELOAD_SUCCESS_NOTICE));
        assert!(!state.take_expired_success_notice(now + HOT_RELOAD_SUCCESS_NOTICE));
    }

    #[test]
    fn slow_prepare_status_is_marked_once_after_delay() {
        let now = Instant::now();
        let mut state = HotReloadState::default();
        state.replace_target(target());
        state.observe(&changed_target(), now);
        let due = now + HOT_RELOAD_DEBOUNCE;
        let revision = state.due_revision(due).unwrap();
        assert!(state.begin_preparing(RenderSessionId(7), revision, due));

        assert!(!state.mark_preparing_slow(due));
        assert!(state.mark_preparing_slow(due + HOT_RELOAD_SLOW_DELAY));
        assert!(!state.mark_preparing_slow(due + HOT_RELOAD_SLOW_DELAY));
    }

    #[test]
    fn watcher_recovery_uses_bounded_backoff_and_resets() {
        let now = Instant::now();
        let mut recovery = WatcherRecoveryState::default();

        recovery.record_failure(now);
        assert!(!recovery.retry_is_due(now));
        assert!(recovery.retry_is_due(now + WATCHER_RECOVERY_DELAYS[0]));

        recovery.record_success();
        assert!(!recovery.retry_is_due(now + Duration::from_secs(60)));
    }

    #[test]
    fn full_callback_channel_requests_reconciliation() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let overflowed = Arc::new(AtomicBool::new(false));
        let mut handler = event_handler(sender, Arc::clone(&overflowed));
        let event = || Ok(Event::new(EventKind::Any).add_path(slides_dir().join("deck.pdf")));

        handler(event());
        handler(event());

        assert!(receiver.try_recv().is_ok());
        assert!(overflowed.load(Ordering::Acquire));
    }

    #[test]
    fn manual_open_suppresses_old_target_events_until_it_finishes() {
        let mut state = HotReloadState::default();
        state.replace_target(target());
        state.begin_manual_open();

        assert!(!state.observe(&changed_target(), Instant::now()));

        state.finish_manual_open_without_replacement();
        assert!(state.observe(&changed_target(), Instant::now()));
    }

    #[test]
    fn cancelling_pending_reload_invalidates_its_revision() {
        let now = Instant::now();
        let mut state = HotReloadState::default();
        state.replace_target(target());
        assert!(state.observe(&changed_target(), now));
        let revision = state.due_revision(now + HOT_RELOAD_DEBOUNCE).unwrap();
        assert!(state.begin_preparing(RenderSessionId(7), revision, now + HOT_RELOAD_DEBOUNCE));

        state.cancel_pending_reload();

        assert_eq!(state.phase(), HotReloadPhase::Idle);
        assert!(!state.accepts_prepared(RenderSessionId(7), revision));
    }
}
